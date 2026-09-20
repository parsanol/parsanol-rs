// Quick A/B: plan scan vs per-byte skip_while, identifier-heavy input.
use std::time::Instant;

fn skip_while_pred(input: &[u8], pos: usize, predicate: impl Fn(u8) -> bool) -> usize {
    let mut current = pos;
    while current < input.len() && predicate(input[current]) {
        current += 1;
    }
    current
}

fn main() {
    let mut input = Vec::with_capacity(4 << 20);
    for i in 0..200_000 {
        let word = format!("identifier_{} ", i % 997);
        input.extend_from_slice(word.as_bytes());
    }
    // word-char plan: ranges + '_' single
    let plan = parsanol::portable::scan::ScanPlan::from_membership(|b| {
        b.is_ascii_alphanumeric() || b == b'_'
    });
    let pred = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut sink = 0usize;
    let t0 = Instant::now();
    let mut pos = 0;
    while pos < input.len() {
        let e = plan.scan_run_bytewise(&input, pos);
        sink += e - pos;
        pos = e + 1; // skip separator
    }
    let t_plan = t0.elapsed();

    let t1 = Instant::now();
    let mut pos2 = 0;
    while pos2 < input.len() {
        let e = skip_while_pred(&input, pos2, pred);
        sink += e.wrapping_sub(pos2);
        pos2 = e + 1;
    }
    let t_skip = t1.elapsed();

    println!(
        "ident runs: plan {:?} skip_while {:?} (sink {})",
        t_plan,
        t_skip,
        sink % 2
    );

    // Long-run corpus (padding/dash blocks): where wide windows pay
    // off most.
    let mut long_input = Vec::with_capacity(4 << 20);
    for i in 0..40_000 {
        long_input.extend_from_slice(format!("{:width$}", i, width = 40).as_bytes());
        long_input.extend_from_slice(&vec![b'-'; 80]);
        long_input.push(b'\n');
    }
    let ws_plan = parsanol::portable::scan::ScanPlan::from_membership(|b| {
        b == b'-' || b == b' ' || b.is_ascii_digit()
    });
    let t_long = {
        let t = std::time::Instant::now();
        let mut pos = 0;
        let mut sink = 0usize;
        while pos < long_input.len() {
            let e = ws_plan.scan_run_bytewise(&long_input, pos);
            sink += e.wrapping_sub(pos);
            pos = e + 1;
        }
        std::hint::black_box(sink);
        t.elapsed()
    };
    let pred = |b: u8| b == b'-' || b == b' ' || b.is_ascii_digit();
    let t_long_scalar = {
        let t = std::time::Instant::now();
        let mut pos = 0;
        let mut sink = 0usize;
        while pos < long_input.len() {
            let mut e = pos;
            while e < long_input.len() && pred(long_input[e]) {
                e += 1;
            }
            sink += e.wrapping_sub(pos);
            pos = e + 1;
        }
        std::hint::black_box(sink);
        t.elapsed()
    };
    println!(
        "long runs: wide plan {:?} scalar {:?}",
        t_long, t_long_scalar
    );
}
