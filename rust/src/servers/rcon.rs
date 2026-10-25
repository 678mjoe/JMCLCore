//! Bounded IPv4-loopback Source RCON. A distinct empty-command sentinel,
//! never an idle timeout, delimits a multi-packet reply.
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
pub const MAX_FRAME: usize = 64 * 1024;
pub const MAX_REPLY: usize = 256 * 1024;
pub type Result<T> = std::result::Result<T, &'static str>;
#[derive(Debug, serde::Serialize)]
pub struct Reply {
    pub text: String,
    pub truncated: bool,
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or("TransportUnavailable")
}
fn read_exact(stream: &mut TcpStream, deadline: Instant, mut bytes: &mut [u8]) -> Result<()> {
    while !bytes.is_empty() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|_| "TransportUnavailable")?;
        let n = stream.read(bytes).map_err(|_| "TransportUnavailable")?;
        if n == 0 {
            return Err("TransportUnavailable");
        }
        bytes = &mut bytes[n..];
    }
    Ok(())
}
fn write_packet(
    stream: &mut TcpStream,
    deadline: Instant,
    id: i32,
    kind: i32,
    body: &str,
) -> Result<()> {
    if body.len() > MAX_FRAME - 10 {
        return Err("ProtocolError");
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(body.len() + 14));
    bytes.extend_from_slice(&((body.len() + 10) as i32).to_le_bytes());
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(body.as_bytes());
    bytes.extend_from_slice(&[0, 0]);
    let mut pending = bytes.as_slice();
    while !pending.is_empty() {
        stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(|_| "TransportUnavailable")?;
        let n = stream.write(pending).map_err(|_| "TransportUnavailable")?;
        if n == 0 {
            return Err("TransportUnavailable");
        }
        pending = &pending[n..];
    }
    Ok(())
}
fn read_packet(stream: &mut TcpStream, deadline: Instant) -> Result<(i32, i32, Vec<u8>)> {
    let mut length = [0; 4];
    read_exact(stream, deadline, &mut length)?;
    let length = i32::from_le_bytes(length);
    if !(10..=MAX_FRAME as i32).contains(&length) {
        return Err("ProtocolError");
    }
    let mut frame = vec![0; length as usize];
    read_exact(stream, deadline, &mut frame)?;
    if !frame.ends_with(&[0, 0]) {
        return Err("ProtocolError");
    }
    Ok((
        i32::from_le_bytes(frame[..4].try_into().unwrap()),
        i32::from_le_bytes(frame[4..8].try_into().unwrap()),
        frame[8..frame.len() - 2].to_vec(),
    ))
}
pub fn execute(port: u16, password: &str, command: &str, max_bytes: usize) -> Result<Reply> {
    if password.is_empty()
        || password.len() > 4095
        || command.is_empty()
        || command.len() > 4095
        || password.contains('\0')
        || command.contains('\0')
        || max_bytes > MAX_REPLY
    {
        return Err("InvalidInput");
    }
    let limit = if max_bytes == 0 { MAX_REPLY } else { max_bytes };
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_secs(3),
    )
    .map_err(|_| "TransportUnavailable")?;
    let deadline = Instant::now() + Duration::from_secs(3);
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let auth = loop {
        let id = NEXT.fetch_add(3, Ordering::Relaxed) & 0x7fff_ffff;
        if id > 0 && id <= 0x7fff_fffc {
            break id as i32;
        }
    };
    let command_id = auth + 1;
    let sentinel = auth + 2;
    write_packet(&mut stream, deadline, auth, 3, password)?;
    loop {
        let (id, kind, body) = read_packet(&mut stream, deadline)?;
        if id == -1 {
            return Err("AuthenticationFailed");
        }
        if id != auth {
            return Err("ProtocolError");
        }
        if kind == 0 && body.is_empty() {
            continue;
        }
        if kind != 2 || !body.is_empty() {
            return Err("ProtocolError");
        }
        break;
    }
    write_packet(&mut stream, deadline, command_id, 2, command)?;
    let (id, kind, body) = read_packet(&mut stream, deadline)?;
    if id != command_id || kind != 0 {
        return Err("ProtocolError");
    }
    let mut bytes = Vec::new();
    let mut truncated = false;
    let append = |bytes: &mut Vec<u8>, body: &[u8], truncated: &mut bool| {
        let n = limit.saturating_sub(bytes.len()).min(body.len());
        bytes.extend_from_slice(&body[..n]);
        *truncated |= n != body.len();
    };
    append(&mut bytes, &body, &mut truncated);
    write_packet(&mut stream, deadline, sentinel, 2, "")?;
    loop {
        let (id, kind, body) = read_packet(&mut stream, deadline)?;
        if kind != 0 {
            return Err("ProtocolError");
        }
        if id == sentinel {
            if !body.is_empty() {
                return Err("ProtocolError");
            }
            break;
        }
        if id != command_id {
            return Err("ProtocolError");
        }
        append(&mut bytes, &body, &mut truncated);
    }
    if std::str::from_utf8(&bytes).is_err() {
        if !truncated {
            return Err("ProtocolError");
        }
        while !bytes.is_empty() && std::str::from_utf8(&bytes).is_err() {
            bytes.pop();
        }
    }
    Ok(Reply {
        text: String::from_utf8(bytes).map_err(|_| "ProtocolError")?,
        truncated,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(auth_fail: bool, limit: usize) -> Result<Reply> {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            let (auth, kind, password) = read_packet(&mut stream, deadline).unwrap();
            assert_eq!(kind, 3);
            assert_eq!(password, b"fixture-only");
            if auth_fail {
                write_packet(&mut stream, deadline, -1, 2, "").unwrap();
                return;
            }
            write_packet(&mut stream, deadline, auth, 0, "").unwrap();
            write_packet(&mut stream, deadline, auth, 2, "").unwrap();
            let (command, kind, body) = read_packet(&mut stream, deadline).unwrap();
            assert_eq!(kind, 2);
            assert_eq!(body, b"list");
            write_packet(&mut stream, deadline, command, 0, "你").unwrap();
            let (sentinel, kind, body) = read_packet(&mut stream, deadline).unwrap();
            assert_ne!(sentinel, command);
            assert_eq!(kind, 2);
            assert!(body.is_empty());
            write_packet(&mut stream, deadline, command, 0, "好").unwrap();
            write_packet(&mut stream, deadline, sentinel, 0, "").unwrap();
        });
        let result = execute(port, "fixture-only", "list", limit);
        server.join().unwrap();
        result
    }
    #[test]
    fn sentinel_multiframe_and_utf8_ceiling() {
        let r = fixture(false, 4).unwrap();
        assert_eq!(r.text, "你");
        assert!(r.truncated);
        let r = fixture(false, 0).unwrap();
        assert_eq!(r.text, "你好");
        assert!(!r.truncated);
    }
    #[test]
    fn auth_failure_and_invalid_input() {
        assert_eq!(fixture(true, 0).unwrap_err(), "AuthenticationFailed");
        assert_eq!(execute(1, "", "list", 1).unwrap_err(), "InvalidInput");
    }
}
