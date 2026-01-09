#[derive(Default)]
pub(crate) struct CsvWriter {
    out: String,
}

impl CsvWriter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push_record<I, S>(&mut self, fields: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut first = true;
        for field in fields {
            if !first {
                self.out.push(',');
            }
            first = false;
            let escaped = escape_csv_field(field.as_ref());
            self.out.push_str(&escaped);
        }
        self.out.push('\n');
    }

    pub(crate) fn push_raw(&mut self, value: &str) {
        self.out.push_str(value);
    }

    pub(crate) fn into_string(self) -> String {
        self.out
    }
}

pub(crate) fn vars_csv(vars: &[String]) -> String {
    if vars.is_empty() {
        return String::new();
    }
    vars.join(",")
}

pub(crate) fn escape_csv_field(value: &str) -> String {
    let needs_quotes =
        value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r');
    if !needs_quotes {
        return value.to_string();
    }
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
