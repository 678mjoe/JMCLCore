//! Bounded GameSpy4 UDP Query, with strict peer, session, framing and field checks.
use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, &'static str>;
const MAX_DATAGRAM: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Basic,
    Full,
}
#[derive(Debug, serde::Serialize)]
pub struct Reply {
    pub mode: Mode,
    pub hostname: String,
    pub game_type: String,
    pub map: String,
    pub version: Option<String>,
    pub players: u32,
    pub max_players: u32,
    pub host_port: u16,
    pub host_ip: String,
    pub plugins: Option<String>,
    pub player_names: Vec<String>,
}
fn string<'a>(bytes: &'a [u8], i: &mut usize) -> Result<&'a str> {
    let tail = bytes.get(*i..).ok_or("ProtocolError")?;
    let end = tail.iter().position(|b| *b == 0).ok_or("ProtocolError")?;
    if end > 1024 {
        return Err("ProtocolError");
    }
    *i += end + 1;
    std::str::from_utf8(&tail[..end]).map_err(|_| "ProtocolError")
}
fn number(s: &str) -> Result<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err("ProtocolError");
    }
    s.parse().map_err(|_| "ProtocolError")
}
fn challenge(packet: &[u8], session: [u8; 4]) -> Result<i32> {
    if packet.len() < 7 || packet[0] != 9 || packet[1..5] != session {
        return Err("ProtocolError");
    }
    let mut i = 5;
    let token = string(packet, &mut i)?;
    if i != packet.len()
        || token.is_empty()
        || token.len() > 16
        || !token.bytes().all(|b| b == b'-' || b.is_ascii_digit())
    {
        return Err("ProtocolError");
    }
    token.parse().map_err(|_| "ProtocolError")
}
fn status(packet: &[u8], session: [u8; 4], mode: Mode) -> Result<Reply> {
    if packet.len() > MAX_DATAGRAM || packet.len() < 7 || packet[0] != 0 || packet[1..5] != session
    {
        return Err("ProtocolError");
    }
    let body = &packet[5..];
    let mut i = 0;
    if mode == Mode::Basic {
        let hostname = string(body, &mut i)?.into();
        let game_type = string(body, &mut i)?.into();
        let map = string(body, &mut i)?.into();
        let players = number(string(body, &mut i)?)?;
        let max_players = number(string(body, &mut i)?)?;
        if players > max_players || max_players > 256 {
            return Err("ProtocolError");
        }
        let port = body.get(i..i + 2).ok_or("ProtocolError")?;
        let host_port = u16::from_le_bytes(port.try_into().unwrap());
        if host_port == 0 {
            return Err("ProtocolError");
        }
        i += 2;
        let host_ip = string(body, &mut i)?.into();
        if i != body.len() {
            return Err("ProtocolError");
        }
        return Ok(Reply {
            mode,
            hostname,
            game_type,
            map,
            version: None,
            players,
            max_players,
            host_port,
            host_ip,
            plugins: None,
            player_names: vec![],
        });
    }
    if !body.starts_with(b"splitnum\0\x80\0") {
        return Err("ProtocolError");
    }
    i = 11;
    let mut fields = std::collections::HashMap::new();
    loop {
        let key = string(body, &mut i)?;
        if key.is_empty() {
            break;
        }
        let value = string(body, &mut i)?;
        if !matches!(
            key,
            "hostname"
                | "gametype"
                | "map"
                | "version"
                | "numplayers"
                | "maxplayers"
                | "hostport"
                | "hostip"
                | "plugins"
                | "game_id"
        ) || fields.insert(key, value).is_some()
            || key == "game_id" && value != "MINECRAFT"
        {
            return Err("ProtocolError");
        }
    }
    if !body[i..].starts_with(b"\x01player_\0\0") {
        return Err("ProtocolError");
    }
    i += 10;
    let mut player_names = vec![];
    loop {
        let name = string(body, &mut i)?;
        if name.is_empty() {
            break;
        }
        if player_names.len() == 256 {
            return Err("ProtocolError");
        }
        player_names.push(name.into());
    }
    if i != body.len() {
        return Err("ProtocolError");
    }
    let required = |name| fields.get(name).copied().ok_or("ProtocolError");
    let players = number(required("numplayers")?)?;
    let max_players = number(required("maxplayers")?)?;
    let port = number(required("hostport")?)?;
    if players > max_players
        || max_players > 256
        || player_names.len() > players as usize
        || port == 0
        || port > 65535
    {
        return Err("ProtocolError");
    }
    Ok(Reply {
        mode,
        hostname: required("hostname")?.into(),
        game_type: required("gametype")?.into(),
        map: required("map")?.into(),
        version: fields.get("version").map(|s| (*s).into()),
        players,
        max_players,
        host_port: port as u16,
        host_ip: required("hostip")?.into(),
        plugins: fields.get("plugins").map(|s| (*s).into()),
        player_names,
    })
}
pub fn execute(port: u16, mode: Mode) -> Result<Reply> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| "TransportUnavailable")?;
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "TransportUnavailable")?;
    let target = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    let session = n.to_be_bytes().map(|b| b & 15);
    let mut handshake = vec![0xfe, 0xfd, 9];
    handshake.extend_from_slice(&session);
    socket
        .send_to(&handshake, target)
        .map_err(|_| "TransportUnavailable")?;
    let mut bytes = [0; MAX_DATAGRAM + 1];
    let (n, peer) = socket
        .recv_from(&mut bytes)
        .map_err(|_| "TransportUnavailable")?;
    if peer != target || n > MAX_DATAGRAM {
        return Err("ProtocolError");
    }
    let token = challenge(&bytes[..n], session)?;
    let mut request = vec![0xfe, 0xfd, 0];
    request.extend_from_slice(&session);
    request.extend_from_slice(&token.to_be_bytes());
    if mode == Mode::Full {
        request.extend_from_slice(&[0; 4]);
    }
    socket
        .send_to(&request, target)
        .map_err(|_| "TransportUnavailable")?;
    let (n, peer) = socket
        .recv_from(&mut bytes)
        .map_err(|_| "TransportUnavailable")?;
    if peer != target || n > MAX_DATAGRAM {
        return Err("ProtocolError");
    }
    status(&bytes[..n], session, mode)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_bounds_and_challenge() {
        let packet = b"\0\x01\x02\x03\x04host\0SMP\0world\x002\x0020\0\xdd\x63127.0.0.1\0";
        let r = status(packet, [1, 2, 3, 4], Mode::Basic).unwrap();
        assert_eq!(r.host_port, 25565);
        assert_eq!(r.players, 2);
        assert_eq!(challenge(b"\x09\x01\x02\x03\x041\0", [1, 2, 3, 4]), Ok(1));
        assert!(challenge(b"\x09\x01\x02\x03\x041\0x", [1, 2, 3, 4]).is_err());
    }
    #[test]
    fn full_strict_fields() {
        let mut p = vec![0, 1, 2, 3, 4];
        p.extend_from_slice(b"splitnum\0\x80\0hostname\0host\0gametype\0SMP\0map\0world\0game_id\0MINECRAFT\0numplayers\x001\0maxplayers\x0020\0hostport\x0025565\0hostip\x00127.0.0.1\0\0\x01player_\0\0Alex\0\0");
        let r = status(&p, [1, 2, 3, 4], Mode::Full).unwrap();
        assert_eq!(r.player_names, ["Alex"]);
        p.push(0);
        assert!(status(&p, [1, 2, 3, 4], Mode::Full).is_err());
    }
    #[test]
    fn udp_session_peer_and_endian() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let thread = std::thread::spawn(move || {
            let mut b = [0; 64];
            let (n, peer) = server.recv_from(&mut b).unwrap();
            assert_eq!(n, 7);
            let session = b[3..7].to_vec();
            let mut reply = vec![9];
            reply.extend_from_slice(&session);
            reply.extend_from_slice(b"-123\0");
            server.send_to(&reply, peer).unwrap();
            let (n, _) = server.recv_from(&mut b).unwrap();
            assert_eq!(n, 11);
            assert_eq!(&b[7..11], &(-123i32).to_be_bytes());
            let mut reply = vec![0];
            reply.extend_from_slice(&session);
            reply.extend_from_slice(b"h\0SMP\0w\x000\x0020\0\xdd\x63ip\0");
            server.send_to(&reply, peer).unwrap();
        });
        assert_eq!(execute(port, Mode::Basic).unwrap().host_port, 25565);
        thread.join().unwrap();
    }
}
