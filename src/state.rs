use gouqi::Issue;

#[derive(Debug)]
pub struct State {
    pub issue: Issue,
}

impl State {
    pub fn new(issue: Issue) -> Self {
        let ret = Self { issue };
        ret.emit();
        ret
    }

    pub fn maybe_update(&mut self, issue: Issue) {
        if self.issue.fields.get("updated") == issue.fields.get("updated") {
            return;
        }

        self.issue = issue;
        self.emit();
    }

    pub fn emit(&self) {
        println!(
            "{}",
            serde_json::to_string(&self.issue).expect("Failed to serialize issue")
        );
    }
}
