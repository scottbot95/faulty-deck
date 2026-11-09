use crate::{boot, state};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::Timer;
use esp_hal::peripherals::LPWR;
use esp_hal::system::software_reset;
use esp_storage::FlashStorage;
use protocol::command::Command;
use static_cell::StaticCell;
use crate::updater::{Updater, CONFIRM};

const CHANNEL_SIZE: usize = 8;
pub static COMMAND_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, Command, CHANNEL_SIZE>> = StaticCell::new();

pub type CmdSender = Sender<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE>;

pub fn spawn_command_task(spawner: &Spawner, flash: FlashStorage<'static>) -> Sender<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE> {
    let channel = COMMAND_CHANNEL.init(Channel::new());

    spawner.spawn(command_task(channel.receiver(), Updater::new(flash)).unwrap());

    channel.sender()
}

#[embassy_executor::task]
async fn command_task(rx: Receiver<'static, CriticalSectionRawMutex, Command, CHANNEL_SIZE>, mut updater: Updater) {
    loop {
        match select(rx.receive(), CONFIRM.wait()).await {
            Either::First(Command::Sync) => {
                state::force_change_signal();
                boot::HOST_SYNCED.signal(());
            }
            Either::First(Command::EnterBootloader(_)) => {
                // Small delay to ensure the host has time to see the ACK for the report that triggered this
                Timer::after_millis(100).await;
                reboot_to_bootloader();
            }
            Either::First(Command::Reboot(_)) => {
                // Small delay to ensure the host has time to see the ACK for the report that triggered this
                Timer::after_millis(100).await;
                software_reset();
            }
            Either::First(command) => updater.handle(command).await,
            Either::Second(()) => updater.confirm(),
        }
    }
}

fn reboot_to_bootloader() -> ! {
    LPWR::regs()
        .option1()
        .write(|w| w.force_download_boot().set_bit());
    esp_hal::system::software_reset()
}
