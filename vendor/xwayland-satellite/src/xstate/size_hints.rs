//! ICCCM preferred buffer sizes, in X pixels rather than host logical units.
use super::{WinSize, WmNormalHints, WmSizeHintsFlags};

impl From<&[u32]> for WmNormalHints {
    fn from(value: &[u32]) -> Self {
        let flags = WmSizeHintsFlags::from_bits_truncate(value.first().copied().unwrap_or(0));
        let pair = |flag, start| {
            if !flags.contains(flag) {
                return None;
            }
            let fields = value.get(start..start + 2)?;
            Some(WinSize {
                width: (fields[0] as i32).max(0),
                height: (fields[1] as i32).max(0),
            })
        };
        Self {
            min_size: pair(WmSizeHintsFlags::ProgramMinSize, 5),
            max_size: pair(WmSizeHintsFlags::ProgramMaxSize, 7),
            resize_increment: pair(WmSizeHintsFlags::ResizeIncrement, 9),
            base_size: pair(WmSizeHintsFlags::BaseSize, 15),
        }
    }
}

impl WmNormalHints {
    pub(crate) fn constrain(self, width: u16, height: u16) -> (u16, u16) {
        let min = self.min_size.or(self.base_size).unwrap_or(WinSize {
            width: 0,
            height: 0,
        });
        let base = self.base_size.unwrap_or(min);
        let max = self.max_size.unwrap_or(WinSize {
            width: 0,
            height: 0,
        });
        let step = self.resize_increment.unwrap_or(WinSize {
            width: 1,
            height: 1,
        });
        (
            axis(width, min.width, max.width, base.width, step.width),
            axis(height, min.height, max.height, base.height, step.height),
        )
    }
}

fn axis(value: u16, min: i32, max: i32, base: i32, step: i32) -> u16 {
    let max = if max > 0 {
        max.min(u16::MAX.into())
    } else {
        u16::MAX.into()
    };
    let min = i64::from(min.clamp(1, max));
    let max = i64::from(max);
    let base = i64::from(base.max(0));
    let step = i64::from(step.max(1));
    let value = i64::from(value).clamp(min, max);
    let first = (min - base + step - 1).div_euclid(step).max(0);
    let last = (max - base).div_euclid(step);
    if first > last {
        return value as u16; // Contradictory hints: bounds take precedence.
    }
    let nearest = ((value - base).max(0) + step / 2) / step;
    (base + nearest.clamp(first, last) * step) as u16
}
