use sqll::FromColumn;

#[derive(FromColumn)]
#[sql(integer)]
enum Locale {
    #[sql(rename = "en")]
    English,
}

#[derive(FromColumn)]
#[sql(text)]
enum Status {
    #[sql(value = 1)]
    Open,
}

#[derive(FromColumn)]
#[sql(integer)]
struct Id(i64);

fn main() {}
