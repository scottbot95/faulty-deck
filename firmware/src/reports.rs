use usbd_hid::descriptor::generator_prelude::*;

#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = VENDOR_DEFINED_START, usage = 0x01) = {
        (usage = 0x01,) = { #[item_settings(data,variable,absolute)] event=input; };
        (usage = 0x02,) = { #[item_settings(data,variable,absolute)] command=output; };
        (usage = 0x03,) = { #[item_settings(data,variable,absolute)] device_info=feature; };
    }
)]
pub struct VendorReport {
    pub event: [u8; 64],
    pub command: [u8; 64],
    pub device_info: [u8; 64],
}
