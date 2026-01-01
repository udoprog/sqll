// Benches copied from https://github.com/stainless-steel/sqlite under the MIT
// license.

use criterion::Criterion;
use sqll::{Connection, Prepare, State};

criterion::criterion_group!(benches, read_statement, write_statement);
criterion::criterion_main!(benches);

fn read_statement(bencher: &mut Criterion) {
    let c = create();

    populate(&c, 100);

    let mut stmt = c
        .prepare_with(
            "SELECT * FROM data WHERE a > ? AND b > ?",
            Prepare::PERSISTENT,
        )
        .unwrap();

    bencher.bench_function("read_statement", |b| {
        b.iter(|| {
            stmt.reset().unwrap();
            stmt.bind(1, 42).unwrap();
            stmt.bind(2, 42.0).unwrap();

            while let State::Row = stmt.step().unwrap() {
                assert!(stmt.get::<i64>(0).unwrap() > 42);
                assert!(stmt.get::<f64>(1).unwrap() > 42.0);
            }
        });
    });
}

fn write_statement(bencher: &mut Criterion) {
    let c = create();

    let mut stmt = c
        .prepare_with(
            "INSERT INTO data (a, b, c, d) VALUES (?, ?, ?, ?)",
            Prepare::PERSISTENT,
        )
        .unwrap();

    bencher.bench_function("write_statement", |b| {
        b.iter(|| {
            stmt.reset().unwrap();
            stmt.bind(1, 42).unwrap();
            stmt.bind(2, 42.0).unwrap();
            stmt.bind(3, 42.0).unwrap();
            stmt.bind(4, 42.0).unwrap();
            assert!(stmt.step().unwrap().is_done());
        });
    });
}

fn create() -> Connection {
    let c = Connection::open(":memory:").unwrap();
    c.execute("CREATE TABLE data (a INTEGER, b REAL, c REAL, d REAL)")
        .unwrap();
    c
}

fn populate(c: &Connection, count: usize) {
    let mut statement = c
        .prepare("INSERT INTO data (a, b, c, d) VALUES (?, ?, ?, ?)")
        .unwrap();

    for i in 0..count {
        statement.reset().unwrap();
        statement.bind(1, i as i64).unwrap();
        statement.bind(2, i as f64).unwrap();
        statement.bind(3, i as f64).unwrap();
        statement.bind(4, i as f64).unwrap();
        assert!(statement.step().unwrap().is_done());
    }
}
