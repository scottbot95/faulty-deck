use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{error, warn, info, info_span, Instrument, debug};
use daemon_core::action::{Action, AudioTarget, Volume, VolumeOp};
use daemon_core::key::KeyCode;

pub trait KeyInjector {

    /// Inject a chord of keys.
    ///
    /// Must press each key in the chord sequentially and then release each key in reverse order.
    ///
    /// **NOTE**: implementors may enter keys simultaneously, but NEVER out-of-order.
    fn chord(&mut self, keys: &[KeyCode]) -> Result<()>;

    /// Enter text (eg like pasting the clipboard).
    fn text(&mut self, text: &str) -> Result<()>;
}

pub trait AudioBackend {
    /// Applies a volume operation to an [`AudioTarget`].
    async fn apply(&self, target: &AudioTarget, op: &VolumeOp) -> Result<()>;
}


pub struct Executor<K: KeyInjector, A: AudioBackend> {
    keys: K,
    audio: A,
    rx: mpsc::Receiver<Action>,
}

impl<K: KeyInjector, A: AudioBackend> Executor<K, A> {
    pub fn new(keys: K, audio: A, rx: mpsc::Receiver<Action>) -> Self {
        Self { keys, audio, rx }
    }

    pub async fn run(mut self) -> Result<()> {
        while let Some(action) = self.rx.recv().await {
            if let Err(e) = self.do_action(&action).await {
                error!("Failed to execute {:?}: {}", action, e);
            }
        }
        Ok(())
    }

    async fn do_action(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Key { chord } => self.keys.chord(chord),
            Action::Text { text } => self.keys.text(text.as_str()),
            Action::Volume { target, op } => {
                self.audio.apply(target, op).await
            },
            Action::Command { program, args, cwd, wait } => {
                run_command(program, args, cwd.as_ref(), *wait).await
            }
            Action::Delay { ms, jitter } => {
                let jitter_bound = (jitter / 2) as i32;
                let sleep = ms.saturating_add_signed(rand::random_range(-jitter_bound..=jitter_bound));
                tokio::time::sleep(Duration::from_millis(sleep as u64)).await;
                Ok(())
            }
            Action::Sequence { actions } => {
                for a in actions {
                    // Need to box to avoid creating a recursive future
                    Box::pin(self.do_action(a)).await?;
                }
                Ok(())
            }
        }
    }

}

async fn forward_lines<R: AsyncRead + Unpin>(reader: R, stream: &'static str) -> std::io::Result<()> {
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        match stream {
            "stderr" => warn!(stream, "{line}"),
            _ => info!(stream, "{line}"),
        }
    }
    Ok(())
}

async fn run_command(program: &PathBuf, args: &[String], cwd: Option<&PathBuf>, wait: bool) -> Result<()> {
    let span = {
        info_span!("run_command", cmd = %program.display())
    };

    let mut cmd = Command::new(program);
    cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);

    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let fut = async move {
        debug!("Running {:?}", cmd);
        let mut child = cmd.spawn()?;

        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stderr = child.stderr.take().context("Failed to capture stderr")?;

        let (status, out, err) = tokio::join!(
            child.wait(),
            forward_lines(stdout, "stdout"),
            forward_lines(stderr, "stderr")
        );
        out?;
        err?;
        let status = status?;
        info!(%status, "command complete");
        Ok(())
    }.instrument(span);

    if wait {
        fut.await
    } else {
        tokio::spawn(fut).await?
    }
}

pub(crate) struct LoggingKeyInjector;
impl KeyInjector for LoggingKeyInjector {
    fn chord(&mut self, keys: &[KeyCode]) -> Result<()> {
        info!("Sending chord: {:?}", keys);
        Ok(())
    }

    fn text(&mut self, text: &str) -> Result<()> {
        info!("Sending text: {:?}", text);
        Ok(())
    }
}

pub(crate) struct LoggingAudio;
impl AudioBackend for LoggingAudio {
    async fn apply(&self, target: &AudioTarget, op: &VolumeOp) -> Result<()> {
        info!("Applying volume operation {:?} to {:?}", op, target);
        Ok(())
    }
}
