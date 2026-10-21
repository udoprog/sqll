use sqll::{SendStatement, Statements};

trait Queries: 'static {
    const INSERT: &'static str;
}

#[derive(Statements)]
struct Write<Q: Queries> {
    #[sql(query = Q::INSERT)]
    insert: SendStatement,
    #[sql(skip)]
    _marker: core::marker::PhantomData<Q>,
}

#[derive(Statements)]
#[sql(read_only)]
struct Read<Q: Queries> {
    #[sql(statements)]
    write: Write<Q>,
}

fn main() {}
