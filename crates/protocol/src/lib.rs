#![no_std]

#[cfg(feature = "std")]
extern crate std;

pub fn reading_to_hue(raw_value: u16) -> u8 {
    ((raw_value as u32 * 240 / 360) >> 4) as u8
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
