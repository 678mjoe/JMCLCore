//! Pure listing seam. Deliberately disconnected from Service fetch/acquisition.
use super::*;

pub struct Listing(Value);
impl Listing {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        super::manifest(bytes).map(Self)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const INPUT: &[u8] = br#"{"latest":{"release":[255],"snapshot":[115]},"versions":[{"id":[255],"type":"release","url":"u","time":"t","releaseTime":"r"}]}"#;
    #[test]
    fn opaque_ids_are_preserved_without_widening_acquisition() {
        let list = Listing::parse(INPUT).unwrap();
        assert_eq!(list.0["latest"]["release"], serde_json::json!([255]));
        assert_eq!(list.0["versions"][0]["id"], serde_json::json!([255]));
        assert!(super::super::manifest(INPUT).is_err());
    }
}
