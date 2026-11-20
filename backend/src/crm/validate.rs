//! Validation of Russian company requisites: INN, KPP, OGRN/OGRNIP, BIK and
//! bank accounts (including the account control key against the BIK).

fn digits(s: &str) -> Option<Vec<u32>> {
    s.chars().map(|c| c.to_digit(10)).collect()
}

fn weighted(d: &[u32], w: &[u32]) -> u32 {
    d.iter().zip(w).map(|(a, b)| a * b).sum()
}

/// INN of a legal entity (10 digits) or an individual (12 digits).
pub fn inn_valid(s: &str) -> bool {
    let Some(d) = digits(s) else { return false };
    match d.len() {
        10 => weighted(&d, &[2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11 % 10 == d[9],
        12 => {
            weighted(&d, &[7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11 % 10 == d[10]
                && weighted(&d, &[3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]) % 11 % 10 == d[11]
        }
        _ => false,
    }
}

/// KPP: 4 digits (tax office), 2 digits or capital latin letters (reason), 3 digits.
pub fn kpp_valid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 9
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4..6].iter().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        && b[6..].iter().all(u8::is_ascii_digit)
}

/// OGRN (13 digits) or OGRNIP (15 digits).
pub fn ogrn_valid(s: &str) -> bool {
    if digits(s).is_none() {
        return false;
    }
    let check = |body: &str, modulo: u64, last: char| {
        body.parse::<u64>().map(|n| (n % modulo % 10) as u32 == last.to_digit(10).unwrap_or(99)).unwrap_or(false)
    };
    match s.len() {
        13 => check(&s[..12], 11, s.chars().nth(12).unwrap_or('x')),
        15 => check(&s[..14], 13, s.chars().nth(14).unwrap_or('x')),
        _ => false,
    }
}

pub fn bik_valid(s: &str) -> bool {
    s.len() == 9 && s.bytes().all(|c| c.is_ascii_digit())
}

/// Control key of a 20-digit account against the bank's BIK.
/// Settlement accounts use the last 3 BIK digits; correspondent accounts use "0" + BIK[4..6].
pub fn account_valid(bik: &str, account: &str, correspondent: bool) -> bool {
    if !bik_valid(bik) || account.len() != 20 || !account.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let prefix = if correspondent { format!("0{}", &bik[4..6]) } else { bik[6..].to_string() };
    let weights = [7u32, 1, 3];
    let sum: u32 = format!("{prefix}{account}")
        .chars()
        .enumerate()
        .map(|(i, c)| c.to_digit(10).unwrap_or(0) * weights[i % 3] % 10)
        .sum();
    sum.is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inn() {
        assert!(inn_valid("7707083893"));
        assert!(!inn_valid("7707083894"));
        assert!(inn_valid("500100732259"));
        assert!(!inn_valid("500100732258"));
        assert!(!inn_valid("12345"));
        assert!(!inn_valid("77070838a3"));
    }

    #[test]
    fn kpp() {
        assert!(kpp_valid("773601001"));
        assert!(kpp_valid("7736AB001"));
        assert!(!kpp_valid("77360100"));
        assert!(!kpp_valid("7736ab001"));
    }

    #[test]
    fn ogrn() {
        assert!(ogrn_valid("1027700132195"));
        assert!(!ogrn_valid("1027700132196"));
        assert!(ogrn_valid("304500116000157"));
        assert!(!ogrn_valid("30450011600015"));
    }

    #[test]
    fn accounts() {
        assert!(account_valid("044525225", "30101810400000000225", true));
        assert!(account_valid("044525225", "40702810938000000001", false));
        assert!(!account_valid("044525225", "40702810038000000001", false));
        assert!(!account_valid("04452522", "40702810938000000001", false));
    }
}
