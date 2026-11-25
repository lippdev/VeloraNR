#[derive(Clone)]
pub struct Field {
    pub group: &'static str,
    pub label: &'static str,
    pub path: &'static str,
    pub min: f32,
    pub max: f32,
    pub initial: f32,
    pub step: f32,
}
pub fn schema() -> Vec<Field> {
    include_str!("../quality.def")
        .lines()
        .map(|line| {
            let line = line
                .strip_prefix("FIELD(\"")
                .unwrap()
                .strip_suffix(')')
                .unwrap();
            let (group, tail) = line.split_once("\", \"").unwrap();
            let (label, tail) = tail.split_once("\", ").unwrap();
            let mut parts = tail.split(", ");
            let path = parts.next().unwrap();
            let mut num = || parts.next().unwrap().parse::<f32>().unwrap();
            Field {
                group,
                label,
                path,
                min: num(),
                max: num(),
                initial: num(),
                step: num(),
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    #[test]
    fn valid_schema() {
        let fields = super::schema();
        let mut paths = std::collections::HashSet::new();
        for f in fields {
            assert!(
                paths.insert(f.path) && f.min <= f.initial && f.initial <= f.max && f.step > 0.
            );
        }
    }
}
