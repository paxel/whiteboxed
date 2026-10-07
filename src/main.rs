fn greeting() -> &'static str {
    "whiteboxed"
}

fn main() {
    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_names_the_app() {
        assert_eq!(greeting(), "whiteboxed");
    }
}
