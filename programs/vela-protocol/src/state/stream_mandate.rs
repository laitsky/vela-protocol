#[cfg(test)]
mod tests {
    use super::StreamMandate;

    #[test]
    fn size_matches_disc_plus_fields() {
        assert_eq!(StreamMandate::SIZE, 225);
    }
}
