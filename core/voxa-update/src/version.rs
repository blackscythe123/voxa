//! `MAJOR.MINOR.PATCH`, compared as numbers (1.10.0 is newer than 1.9.0). No prerelease or build
//! suffixes: a feed never offers one. Android compares `versionCode` instead (see policy).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Strict: three dot-separated decimal numbers, no sign, no leading zeros, no suffix.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let mut next = || -> Option<u32> {
            let p = parts.next()?;
            if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) || (p.len() > 1 && p.starts_with('0')) {
                return None;
            }
            p.parse().ok()
        };
        let v = Self(next()?, next()?, next()?);
        if parts.next().is_some() {
            return None;
        }
        Some(v)
    }

    /// Android's `versionCode` scheme: major*10000 + minor*100 + patch (1.0.0 is 10000).
    pub fn android_code(self) -> u64 {
        u64::from(self.0) * 10000 + u64::from(self.1) * 100 + u64::from(self.2)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers_not_text() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("1.10.0") > v("1.9.0"));
        assert!(v("2.0.0") > v("1.99.99"));
        assert!(v("1.0.10") > v("1.0.9"));
        assert!(v("10.0.0") > v("9.9.9"));
        assert_eq!(v("1.0.0"), Version(1, 0, 0));
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.0.0").android_code(), 10000);
        assert_eq!(v("1.2.3").android_code(), 10203);
    }

    #[test]
    fn a_malformed_or_prerelease_version_is_rejected() {
        for bad in ["", "1", "1.0", "1.0.0.0", "1.0.0-beta", "1.0.0+5", "v1.0.0", " 1.0.0", "1..0", "01.0.0", "-1.0.0", "1.0.x", "99999999999.0.0", "１.0.0"] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert!(Version::parse("0.4.7").is_some());
    }
}
