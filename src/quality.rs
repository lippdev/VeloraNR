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
pub fn fg_limit(backend: u32) -> u32 {
    match backend {
        1 => 4,
        2 | 3 | 6 => 2,
        4 => 8,
        7 => 3,
        _ => 6,
    }
}

pub fn aa_quality_limit(method: u32) -> u32 {
    match method {
        0 => 2,
        1 => 3,
        _ => 0,
    }
}

pub fn normalize(fields: &[Field], values: &mut [f32]) {
    let index = |path| fields.iter().position(|field| field.path == path).unwrap();
    let backend = values[index("frameGenerationBackend")] as u32;
    let multiplier = index("multiplier");
    values[multiplier] = values[multiplier].min(fg_limit(backend) as f32);
    let aa_method = values[index("antiAlias.method")] as u32;
    let aa_quality = index("antiAlias.quality");
    values[aa_quality] = values[aa_quality].min(aa_quality_limit(aa_method) as f32);
}

#[cfg(test)]
mod tests {
    #[test]
    fn switching_methods_keeps_settings_valid() {
        let fields = super::schema();
        let mut values: Vec<_> = fields.iter().map(|field| field.initial).collect();
        let index = |path| fields.iter().position(|field| field.path == path).unwrap();
        values[index("frameGenerationBackend")] = 1.;
        values[index("multiplier")] = 8.;
        values[index("antiAlias.method")] = 2.;
        values[index("antiAlias.quality")] = 3.;
        super::normalize(&fields, &mut values);
        assert_eq!(values[index("multiplier")], 4.);
        assert_eq!(values[index("antiAlias.quality")], 0.);
    }

    #[test]
    fn external_generators_limit_saved_internal_multiplier() {
        let fields = super::schema();
        let mut values: Vec<_> = fields.iter().map(|field| field.initial).collect();
        let index = |path| fields.iter().position(|field| field.path == path).unwrap();
        let backend = index("frameGenerationBackend");
        assert!(fields[backend].max >= 11.);
        for id in 9..=11 {
            values[backend] = id as f32;
            values[index("multiplier")] = 8.;
            super::normalize(&fields, &mut values);
            assert_eq!(values[index("multiplier")], 6.);
        }
    }

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
