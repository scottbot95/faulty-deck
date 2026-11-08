use std::time::{Duration, Instant};
use anyhow::{anyhow, Result, Context as _};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{Context, FlagSet, State};
use libpulse_binding::context::introspect::Introspector;
use libpulse_binding::mainloop::threaded::Mainloop;
use libpulse_binding::proplist::{properties, Proplist};
use libpulse_binding::volume::{ChannelVolumes, Volume as PaVolume};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;
use daemon_core::action::{AudioTarget, Volume, VolumeOp};
use crate::executor::AudioBackend;

enum Command {
    Apply {
        target: AudioTarget,
        op: VolumeOp,
        reply: oneshot::Sender<Result<()>>,
    },
}

impl Command {
    fn fail(self, err: anyhow::Error) {
        match self {
            Command::Apply { reply, .. } => {
                let _ = reply.send(Err(err));
            }
        }
    }
}

/// [`AudioBackend`] implementation backed by libpulse.
///
/// libpulse is a C library which makes heavy use of callbacks for asynchronous operations.
/// Since many of the libpulse types are not actually thread-safe (notably [`Mainloop`]), we cannot
/// just simply use tokio channels to achieve "normal" async semantics. To that end the general
/// design of `PulseAudioBackend` as follows:
/// - Spawn a blocking, non-tokio background thread which owns the [`Mainloop`].
/// - The `PulseAudioBackend` communicates commands and receives replies via tokio channels.
/// - The event loop thread then sends a command to libpulse with a callback that uses
///   the `std` blocking channels to send the reply back to the mainloop thread.
///
/// The mainloop thread using blocking channels *should* be fine since the commands we use are
/// generally quick. If that changes we made need to adapt this design.
pub struct PulseAudioBackend {
    tx: mpsc::Sender<Command>,
}

impl PulseAudioBackend {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(32);
        std::thread::Builder::new()
            .name("pulse".into())
            .spawn(move || owner(rx))
            .unwrap();
        Self { tx }
    }

    /// Helper to create a [`Command`], execute it, and away the reply
    async fn call<T>(&self, f: impl FnOnce(oneshot::Sender<Result<T>>) -> Command) -> Result<T> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(f(reply)).await?;
        rx.await.map_err(|_| anyhow::anyhow!("pulseaudio backend dropped"))?
    }
}

impl AudioBackend for PulseAudioBackend {
    async fn apply(&self, target: &AudioTarget, op: &VolumeOp) -> Result<()> {
        self.call(|reply| Command::Apply { target: target.clone(), op: *op, reply }).await
    }
}

fn owner(mut rx: mpsc::Receiver<Command>) {
    let mut conn: Option<Conn> = None;
    while let Some(cmd) = rx.blocking_recv() {
        if !conn.as_mut().is_some_and(Conn::alive) {
            conn = Conn::connect().inspect_err(|e| warn!("Failed to connect to pulseaudio: {}", e)).ok();
        }
        match (conn.as_mut(), cmd) {
            (Some(c), Command::Apply { target, op, reply }) => {
                let result = c.apply(&target, &op);
                let _ = reply.send(result);
            }
            (None, cmd) => {
                cmd.fail(anyhow!("Failed to connect to pulseaudio"))
            }
        }
    }
}

struct Conn {
    ml: Mainloop,
    ctx: Context,
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.ml.lock();
        self.ctx.disconnect();
        self.ml.unlock();
        self.ml.stop();
    }
}

const OP_TIMEOUT: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

// Connection creation
impl Conn {
    fn connect() -> Result<Self> {
        // Create the libpulse handles

        let mut props = Proplist::new()
            .ok_or_else(|| anyhow!("Failed to create pulseaudio property list"))?;
        props.set_str(properties::APPLICATION_NAME, "faultydeck")
            .map_err(|_| anyhow!("Failed to set application name"))?;

        let mut ml = Mainloop::new()
            .ok_or_else(|| anyhow!("Failed to create pulseaudio mainloop"))?;
        let mut ctx = Context::new_with_proplist(&ml, "faultydeck", &props)
            .ok_or_else(|| anyhow!("Failed to create pulseaudio context"))?;

        // Actually open a connection
        ml.lock();
        let r = ctx.connect(None, FlagSet::NOFLAGS, None);
        ml.unlock();
        r.context("Failed to connect to pulseaudio")?;
        ml.start().context("Failed to start pulseaudio mainloop")?;

        // Poll until the connection has been established
        let deadline = Instant::now() + CONNECT_TIMEOUT;
        loop {
            ml.lock();
            let state = ctx.get_state();
            ml.unlock();
            match state {
                State::Ready => break,
                State::Failed
                | State::Terminated => return Err(anyhow!("Connection to pulseaudio failed")),
                _ if Instant::now() > deadline => return Err(anyhow!("Connection to pulseaudio timed out")),
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        Ok(Self { ml, ctx })
    }

    /// Checks if the connection to pulseaudio is alive
    fn alive(&mut self) -> bool {
        self.ml.lock();
        let s = self.ctx.get_state();
        self.ml.unlock();
        matches!(s, State::Ready)
    }

    /// `issue` registers a libpulse operation whose callback sends the result on `tx`.
    fn op<T: Send + 'static>(
        &mut self,
        issue: impl FnOnce(&mut Introspector, std::sync::mpsc::Sender<T>)
    ) -> Result<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.ml.lock();
        issue(&mut self.ctx.introspect(), tx);
        self.ml.unlock();
        rx.recv_timeout(OP_TIMEOUT)
            .context("Timeout waiting for operation result")
    }
}

// ================ high-level interface =====================
impl Conn {
    fn apply(&mut self, target: &AudioTarget, op: &VolumeOp) -> Result<()> {
        match op {
            VolumeOp::Set(v) => self.set_volume(target, *v),
            VolumeOp::ToggleMute => self.toggle_mute(target),
        }
    }

    fn set_volume(&mut self, target: &AudioTarget, v: Volume) -> Result<()> {
        match target {
            AudioTarget::Sink(n) => {
                let cur = self.sink_level(n)?;
                self.set_sink_volume(n, &with_level(cur.volume, v))
            }
            AudioTarget::Source(n) => {
                let cur = self.source_level(n)?;
                self.set_source_volume(n, &with_level(cur.volume, v))
            }
        }
    }

    fn set_mute(&mut self, target: &AudioTarget, mute: bool) -> Result<()> {
        match target {
            AudioTarget::Sink(n) => self.set_sink_mute(n, mute),
            AudioTarget::Source(n) => self.set_source_mute(n, mute),
        }
    }

    fn toggle_mute(&mut self, target: &AudioTarget) -> Result<()> {
        let cur = self.level(target)?;
        self.set_mute(target, !cur.mute)
    }

    fn level(&mut self, target: &AudioTarget) -> Result<Level> {
        match target {
            AudioTarget::Sink(n) => self.sink_level(n),
            AudioTarget::Source(n) => self.source_level(n),
        }
    }
}

// ================ libpulse primitives =====================
struct Level {
    volume: ChannelVolumes,
    mute: bool,
}

/// Produces a simple callback that will report success or failure back to `tx`
fn success_callback(tx: std::sync::mpsc::Sender<bool>) -> Box<dyn FnMut(bool)>{
    Box::new(move |ok| {
        let _ = tx.send(ok);
    })
}

impl Conn {
    // Sink functions
    fn sink_level(&mut self, name: &str) -> Result<Level> {
        self.op(|i, tx| {
            i.get_sink_info_by_name(name, move |r| match r {
                ListResult::Item(s) => {
                    let _ = tx.send(Some(Level { volume: s.volume, mute: s.mute }));
                }
                ListResult::End | ListResult::Error => {
                    let _ = tx.send(None);
                }
            });
        })?.ok_or_else(|| anyhow::anyhow!("No sink found with name {}", name))
    }

    fn set_sink_volume(&mut self, name: &str, cv: &ChannelVolumes) -> Result<()> {
        self.op(|i, tx| {
            i.set_sink_volume_by_name(name, cv, Some(success_callback(tx)));
        })?.ok_or_else(|| anyhow::anyhow!("No sink found with name {}", name))
    }

    fn set_sink_mute(&mut self, name: &str, mute: bool) -> Result<()> {
        self.op(|i, tx| {
            i.set_sink_mute_by_name(name, mute, Some(success_callback(tx)));
        })?.ok_or_else(|| anyhow::anyhow!("No sink found with name {}", name))
    }

    // source functions
    fn source_level(&mut self, name: &str) -> Result<Level> {
        self.op(|i, tx| {
            i.get_source_info_by_name(name, move |r| match r {
                ListResult::Item(s) => {
                    let _ = tx.send(Some(Level { volume: s.volume, mute: s.mute }));
                }
                ListResult::End | ListResult::Error => {
                    let _ = tx.send(None);
                }
            });
        })?.ok_or_else(|| anyhow::anyhow!("No source found with name {}", name))
    }

    fn set_source_volume(&mut self, name: &str, cv: &ChannelVolumes) -> Result<()> {
        self.op(|i, tx| {
            i.set_source_volume_by_name(name, cv, Some(success_callback(tx)));
        })?.ok_or_else(|| anyhow::anyhow!("No source found with name {}", name))
    }

    fn set_source_mute(&mut self, name: &str, mute: bool) -> Result<()> {
        self.op(|i, tx| {
            i.set_source_mute_by_name(name, mute, Some(success_callback(tx)));
        })?.ok_or_else(|| anyhow::anyhow!("No source found with name {}", name))
    }
}

// ============== pure helpers =============

/// Sets the volume of all channels in `cv` to `v`
fn with_level(mut cv: ChannelVolumes, v: Volume) -> ChannelVolumes {
    let pa = PaVolume((PaVolume::NORMAL.0 as f32 * v.clamp(0.0, 1.0)).round() as u32);
    cv.set(cv.len(), pa);
    cv
}
