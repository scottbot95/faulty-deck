use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use static_cell::StaticCell;
use protocol::Command;
use crate::state;

const CHANNEL_SIZE: usize = 8;
pub static COMMAND_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, Command, CHANNEL_SIZE>> = StaticCell::new();

pub type CmdSender = Sender<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE>;

pub fn spawn_command_task(spawner: &Spawner) -> Sender<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE> {
    let channel = COMMAND_CHANNEL.init(Channel::new());

    spawner.spawn(command_task(channel.receiver()).unwrap());

    channel.sender()
}

#[embassy_executor::task]
async fn command_task(rx: Receiver<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE>) {
    loop {
        match rx.receive().await {
            Command::Sync => {
                state::force_change_signal();
            }
        }
    }
}
