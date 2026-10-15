//! Tempo from Mixxx. Mixxx can't send a steady MIDI clock (its script timers
//! bottom out at 20 ms, too coarse for 24 pulses per beat), so
//! `docs/mixxx/LumenDMX-Tempo.js` sends the BPM as a 14-bit control change
//! pair and a note on every beat, all on MIDI channel 16:
//!
//! - CC 20 (MSB) then CC 52 (LSB): the BPM × 10 as a 14-bit number
//! - note 0, velocity > 0: a beat happened (aligns the beat grid)

use super::mtc::ClockEvent;

const CHANNEL_16: u8 = 0x0F;
const CC_BPM_MSB: u8 = 20;
const CC_BPM_LSB: u8 = 52;

#[derive(Debug, Default)]
pub struct MixxxDecoder {
    msb: Option<u8>,
}

impl MixxxDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, msg: &[u8]) -> Option<ClockEvent> {
        match *msg {
            [status, CC_BPM_MSB, value] if status == 0xB0 | CHANNEL_16 => {
                self.msb = Some(value & 0x7F);
                None
            }
            [status, CC_BPM_LSB, value] if status == 0xB0 | CHANNEL_16 => {
                let msb = self.msb.take()?;
                let tenths = ((msb as u16) << 7) | (value & 0x7F) as u16;
                (tenths > 0).then(|| ClockEvent::Bpm(tenths as f32 / 10.0))
            }
            [status, 0, velocity] if status == 0x90 | CHANNEL_16 && velocity > 0 => Some(ClockEvent::Start),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_arrives_as_msb_then_lsb() {
        let mut dec = MixxxDecoder::new();
        let tenths = 1743u16; // 174.3 BPM
        assert_eq!(dec.feed(&[0xBF, 20, (tenths >> 7) as u8]), None);
        assert_eq!(dec.feed(&[0xBF, 52, (tenths & 0x7F) as u8]), Some(ClockEvent::Bpm(174.3)));
    }

    #[test]
    fn lsb_without_msb_is_ignored() {
        assert_eq!(MixxxDecoder::new().feed(&[0xBF, 52, 5]), None);
    }

    #[test]
    fn beat_note_aligns_and_other_channels_are_ignored() {
        let mut dec = MixxxDecoder::new();
        assert_eq!(dec.feed(&[0x9F, 0, 127]), Some(ClockEvent::Start));
        assert_eq!(dec.feed(&[0x9F, 0, 0]), None);
        assert_eq!(dec.feed(&[0x90, 0, 127]), None);
        assert_eq!(dec.feed(&[0xB0, 20, 5]), None);
    }
}
