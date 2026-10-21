use sqll::BindValue;

#[derive(BindValue)]
#[sql(integer, nope)]
enum Level {
    Low,
}

#[derive(BindValue)]
#[sql(text, rename_all = "camelCase")]
enum Mode {
    Fast,
}

fn main() {}
