pub fn estimate(text: &str) -> usize {
    (text.len() as f64 / 4.0).ceil() as usize
}

pub fn fmt(n: usize) -> String {
    if n >= 1_000 {
        format!("~{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("~{}", n)
    }
}
