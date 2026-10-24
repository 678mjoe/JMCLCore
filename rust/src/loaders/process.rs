use super::{Error, Runtime, detect};
use std::{
    ffi::OsString,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
#[derive(Default)]
pub struct SystemRuntime {
    stderr: Vec<u8>,
    exit_code: Option<i32>,
}
impl Runtime for SystemRuntime {
    fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
    fn stderr(&self) -> &[u8] {
        &self.stderr
    }
    fn resolve(
        &mut self,
        env: &detect::Environment,
        paths: &[String],
        major: u16,
        arch: &str,
    ) -> Result<String, Error> {
        let detection = detect::detect(env, paths).map_err(Error::named)?;
        if major != 0
            && let Ok(java) = detect::select_compatible(&detection, major, arch)
        {
            return Ok(java.executable);
        }
        detection
            .runtimes
            .into_iter()
            .find(|r| r.major_version >= 8)
            .map(|r| r.executable)
            .ok_or_else(|| Error::named("NoCompatibleJava"))
    }
    fn execute(
        &mut self,
        executable: &str,
        argv: &[OsString],
        cwd: &Path,
        env: &detect::Environment,
    ) -> Result<(), Error> {
        self.stderr.clear();
        self.exit_code = None;
        let mut child = Command::new(executable)
            .args(argv)
            .current_dir(cwd)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::channel();
        fn collect(
            pipe: impl Read + Send + 'static,
            is_stderr: bool,
            tx: mpsc::Sender<Result<(bool, Vec<u8>), Error>>,
        ) {
            std::thread::spawn(move || {
                let mut pipe = pipe.take(16 * 1024 * 1024 + 1);
                let mut bytes = Vec::new();
                let n = pipe.read_to_end(&mut bytes).map_err(Error::from);
                let result = n.and_then(|n| {
                    if n > 16 * 1024 * 1024 {
                        Err(Error::named("StreamTooLong"))
                    } else {
                        Ok((is_stderr, bytes))
                    }
                });
                let _ = tx.send(result);
            });
        }
        collect(stdout, false, tx.clone());
        collect(stderr, true, tx);
        let result = (|| {
            let mut done = 0;
            let mut status = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(2)) {
                    Ok(result) => {
                        let (stderr, bytes) = result?;
                        if stderr {
                            self.stderr = bytes
                        }
                        done += 1
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                if status.is_none() {
                    status = child.try_wait()?;
                }
                if done == 2
                    && let Some(status) = status
                {
                    self.exit_code = status.code();
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(Error::named("ProcessorFailed"))
                    };
                }
            }
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
}
