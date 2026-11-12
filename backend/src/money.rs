//! Money helpers for Russian business documents: amounts in words
//! ("Один миллион двести тысяч рублей 50 копеек") and VAT included in a price.

/// VAT rates accepted for documents: "none" (not a VAT payer) or a percentage.
pub const VAT_RATES: &[&str] = &["none", "0", "5", "7", "10", "20", "22"];

pub fn valid_vat(rate: &str) -> bool {
    VAT_RATES.contains(&rate)
}

/// VAT included in `amount` (price with VAT), rounded to kopecks.
pub fn vat_included(amount: f64, rate: &str) -> f64 {
    let r: f64 = match rate.parse() {
        Ok(r) => r,
        Err(_) => return 0.0,
    };
    round2(amount * r / (100.0 + r))
}

/// «1 234 567» or «1 234,50» (narrow no-break spaces between thousands).
pub fn format_rub(v: f64) -> String {
    let cents = (v.abs() * 100.0).round() as u64;
    let (int, frac) = (cents / 100, cents % 100);
    let digits = int.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{202F}');
        }
        out.push(ch);
    }
    if frac > 0 {
        out.push_str(&format!(",{frac:02}"));
    }
    if v < 0.0 { format!("−{out}") } else { out }
}

pub fn round2(v: f64) -> f64 {
    // `+ 0.0` turns a negative zero into zero (shown as "-0" otherwise).
    (v * 100.0).round() / 100.0 + 0.0
}

/// "В том числе НДС 22% — 270 491,80 руб." / "НДС не облагается".
pub fn vat_text(rate: &str, vat: f64, fmt: impl Fn(f64) -> String) -> String {
    match rate {
        "none" => "НДС не облагается".into(),
        r => format!("В том числе НДС {r}% — {} руб.", fmt(vat)),
    }
}

const UNITS_M: [&str; 10] = ["", "один", "два", "три", "четыре", "пять", "шесть", "семь", "восемь", "девять"];
const UNITS_F: [&str; 10] = ["", "одна", "две", "три", "четыре", "пять", "шесть", "семь", "восемь", "девять"];
const TEENS: [&str; 10] = [
    "десять", "одиннадцать", "двенадцать", "тринадцать", "четырнадцать", "пятнадцать", "шестнадцать", "семнадцать",
    "восемнадцать", "девятнадцать",
];
const TENS: [&str; 10] = ["", "", "двадцать", "тридцать", "сорок", "пятьдесят", "шестьдесят", "семьдесят", "восемьдесят", "девяносто"];
const HUNDREDS: [&str; 10] = ["", "сто", "двести", "триста", "четыреста", "пятьсот", "шестьсот", "семьсот", "восемьсот", "девятьсот"];

/// Russian plural form: 1 рубль, 2 рубля, 5 рублей.
pub fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let n100 = n % 100;
    let n10 = n % 10;
    if (11..=14).contains(&n100) {
        many
    } else if n10 == 1 {
        one
    } else if (2..=4).contains(&n10) {
        few
    } else {
        many
    }
}

fn triad(n: u64, feminine: bool, out: &mut Vec<&'static str>) {
    let h = (n / 100) as usize;
    let t = ((n / 10) % 10) as usize;
    let u = (n % 10) as usize;
    if h > 0 {
        out.push(HUNDREDS[h]);
    }
    if t == 1 {
        out.push(TEENS[u]);
        return;
    }
    if t > 1 {
        out.push(TENS[t]);
    }
    if u > 0 {
        out.push(if feminine { UNITS_F[u] } else { UNITS_M[u] });
    }
}

/// Integer in words, masculine by default ("двадцать один").
pub fn number_in_words(n: u64, feminine: bool) -> String {
    if n == 0 {
        return "ноль".into();
    }
    // (divisor, feminine, forms)
    let scales: [(u64, bool, [&str; 3]); 4] = [
        (1_000_000_000_000, false, ["триллион", "триллиона", "триллионов"]),
        (1_000_000_000, false, ["миллиард", "миллиарда", "миллиардов"]),
        (1_000_000, false, ["миллион", "миллиона", "миллионов"]),
        (1_000, true, ["тысяча", "тысячи", "тысяч"]),
    ];
    let mut words: Vec<&str> = Vec::new();
    let mut rest = n;
    for (div, fem, forms) in scales {
        let part = rest / div;
        rest %= div;
        if part > 0 {
            triad(part % 1000, fem, &mut words);
            words.push(plural(part % 1000, forms[0], forms[1], forms[2]));
        }
    }
    triad(rest, feminine, &mut words);
    words.join(" ")
}

/// "Один миллион двести пятьдесят тысяч рублей 50 копеек".
pub fn rubles_in_words(amount: f64) -> String {
    let total = (amount.abs() * 100.0).round() as u64;
    let rub = total / 100;
    let kop = total % 100;
    let words = number_in_words(rub, false);
    let mut s = format!("{words} {} {kop:02} {}", plural(rub, "рубль", "рубля", "рублей"), plural(kop, "копейка", "копейки", "копеек"));
    if amount < 0.0 {
        s = format!("минус {s}");
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(rubles_in_words(0.0), "Ноль рублей 00 копеек");
        assert_eq!(rubles_in_words(1.0), "Один рубль 00 копеек");
        assert_eq!(rubles_in_words(2.01), "Два рубля 01 копейка");
        assert_eq!(rubles_in_words(11.12), "Одиннадцать рублей 12 копеек");
        assert_eq!(rubles_in_words(21.22), "Двадцать один рубль 22 копейки");
        assert_eq!(rubles_in_words(1000.0), "Одна тысяча рублей 00 копеек");
        assert_eq!(rubles_in_words(2002.0), "Две тысячи два рубля 00 копеек");
        assert_eq!(rubles_in_words(11_000.0), "Одиннадцать тысяч рублей 00 копеек");
        assert_eq!(rubles_in_words(1_250_000.5), "Один миллион двести пятьдесят тысяч рублей 50 копеек");
        assert_eq!(rubles_in_words(4_800_000.0), "Четыре миллиона восемьсот тысяч рублей 00 копеек");
        assert_eq!(
            rubles_in_words(123_456_789.99),
            "Сто двадцать три миллиона четыреста пятьдесят шесть тысяч семьсот восемьдесят девять рублей 99 копеек"
        );
        assert_eq!(rubles_in_words(2_000_000_000.0), "Два миллиарда рублей 00 копеек");
        assert_eq!(number_in_words(112, false), "сто двенадцать");
    }

    #[test]
    fn vat() {
        assert_eq!(vat_included(1_220_000.0, "22"), 220_000.0);
        assert_eq!(vat_included(120.0, "20"), 20.0);
        assert_eq!(vat_included(1000.0, "none"), 0.0);
        assert_eq!(vat_included(1000.0, "0"), 0.0);
        assert_eq!(vat_included(100.0, "7"), 6.54);
        assert!(valid_vat("22") && !valid_vat("18"));
        assert_eq!(vat_text("none", 0.0, |v| v.to_string()), "НДС не облагается");
    }
}
