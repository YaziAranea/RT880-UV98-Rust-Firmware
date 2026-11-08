//! Semantic zone membership for the CPS display, separate from lossless raw
//! codeplug access. Inspecting a record never repairs its header or member words.

use uv98_codeplug::zones::{Zone, EMPTY_MEMBER, MEMBER_COUNT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneDisplayError {
    HeaderMismatch { first: u8, second: u8 },
    HeaderCount { declared: u8 },
    MemberCode { slot: usize, code: u16 },
    CountMismatch { declared: u8, found: usize },
}

/// Returns zero-based channel indices in their stored order, including repeats.
/// A duplicated zero count means an empty zone even if unused storage is filled
/// with zeroes. Nonempty records must have valid, matching count bytes and member
/// words; ambiguous records stay available through the raw codeplug API only.
pub fn zone_display_members(zone: Zone<'_>) -> Result<Vec<u16>, ZoneDisplayError> {
    let [first, second] = zone.cps_count_bytes();
    if first != second {
        return Err(ZoneDisplayError::HeaderMismatch { first, second });
    }
    if usize::from(first) > MEMBER_COUNT {
        return Err(ZoneDisplayError::HeaderCount { declared: first });
    }
    if first == 0 {
        return Ok(Vec::new());
    }
    let mut members = Vec::with_capacity(usize::from(first));
    for slot in 0..MEMBER_COUNT {
        let code = zone.member_code(slot).expect("bounded member slot");
        if code == EMPTY_MEMBER {
            continue;
        }
        if code >= 1024 {
            return Err(ZoneDisplayError::MemberCode { slot, code });
        }
        members.push(code);
    }
    if members.len() != usize::from(first) {
        return Err(ZoneDisplayError::CountMismatch {
            declared: first,
            found: members.len(),
        });
    }
    Ok(members)
}
