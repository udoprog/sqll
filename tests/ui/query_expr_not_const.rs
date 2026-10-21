use sqll::{SendStatement, Statements};

fn not_const() -> &'static str {
    "SELECT 1"
}

#[derive(Statements)]
struct NotConst {
    #[sql(query = not_const())]
    field: SendStatement,
}

fn main() {}
