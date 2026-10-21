use sqll::BindValue;

#[derive(BindValue)]
#[sql(integer)]
enum Shape {
    Point,
    Circle(f64),
}

fn main() {}
