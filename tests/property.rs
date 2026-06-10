// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

//! Property-style tests over many generated *valid* DNA PSL records, using a
//! deterministic LCG so failures reproduce. Asserts that every generated record
//! passes `check`, round-trips through the writer, and satisfies
//! `swap(swap(x)) == x`.

use psltools::{Psl, PslRecord, Reader, check, swap};

/// A tiny deterministic PRNG (LCG) so the suite is reproducible without a dep.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() as u32) % (hi - lo)
    }
    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// Builds the text of one valid DNA PSL record.
fn generate(rng: &mut Lcg) -> String {
    let minus = rng.bool();
    let n = rng.range(1, 6) as usize;

    let mut sizes = Vec::with_capacity(n);
    let mut q_gaps = Vec::with_capacity(n.saturating_sub(1));
    let mut t_gaps = Vec::with_capacity(n.saturating_sub(1));
    for i in 0..n {
        sizes.push(rng.range(1, 50));
        if i + 1 < n {
            q_gaps.push(rng.range(0, 10));
            t_gaps.push(rng.range(0, 10));
        }
    }

    // Raw (on-disk) start lists, strictly increasing in alignment order.
    let q_base = rng.range(0, 20);
    let t_base = rng.range(0, 20);
    let mut q_starts = Vec::with_capacity(n);
    let mut t_starts = Vec::with_capacity(n);
    let (mut qc, mut tc) = (q_base, t_base);
    for (i, &size) in sizes.iter().enumerate() {
        q_starts.push(qc);
        t_starts.push(tc);
        qc += size + q_gaps.get(i).copied().unwrap_or(0);
        tc += size + t_gaps.get(i).copied().unwrap_or(0);
    }

    let sum_sizes: u32 = sizes.iter().sum();
    let q_base_insert: u32 = q_gaps.iter().sum();
    let t_base_insert: u32 = t_gaps.iter().sum();
    let q_num_insert = q_gaps.iter().filter(|&&g| g > 0).count() as u32;
    let t_num_insert = t_gaps.iter().filter(|&&g| g > 0).count() as u32;

    let raw_q_end = q_starts[n - 1] + sizes[n - 1];
    let raw_t_end = t_starts[n - 1] + sizes[n - 1];
    let q_tail = rng.range(0, 30);
    let t_tail = rng.range(0, 30);
    let q_size = raw_q_end + q_tail;
    let t_size = raw_t_end + t_tail;

    // Forward qStart/qEnd. The reference is always + here.
    let (q_start, q_end) = if minus {
        (q_size - raw_q_end, q_size - q_starts[0])
    } else {
        (q_starts[0], raw_q_end)
    };
    let t_start = t_starts[0];
    let t_end = raw_t_end;

    let strand = if minus { "-" } else { "+" };
    let list = |xs: &[u32]| {
        let mut s = String::new();
        for x in xs {
            s.push_str(&x.to_string());
            s.push(',');
        }
        s
    };

    format!(
        "{matches}\t0\t0\t0\t{qni}\t{qbi}\t{tni}\t{tbi}\t{strand}\tq\t{qsize}\t{qstart}\t{qend}\tc\t{tsize}\t{tstart}\t{tend}\t{n}\t{bs}\t{qs}\t{ts}\n",
        matches = sum_sizes,
        qni = q_num_insert,
        qbi = q_base_insert,
        tni = t_num_insert,
        tbi = t_base_insert,
        qsize = q_size,
        qstart = q_start,
        qend = q_end,
        tsize = t_size,
        tstart = t_start,
        tend = t_end,
        bs = list(&sizes),
        qs = list(&q_starts),
        ts = list(&t_starts),
    )
}

fn single(text: &str) -> Psl {
    let reader = Reader::<Psl>::from_owned_bytes(text.as_bytes().to_vec()).expect("parse");
    assert_eq!(reader.len(), 1, "generated record did not parse: {text}");
    reader.as_slice()[0].clone()
}

fn structurally_equal<A: PslRecord, B: PslRecord>(a: &A, b: &B) -> bool {
    a.matches() == b.matches()
        && a.mismatches() == b.mismatches()
        && a.rep_matches() == b.rep_matches()
        && a.n_count() == b.n_count()
        && a.query_num_insert() == b.query_num_insert()
        && a.query_base_insert() == b.query_base_insert()
        && a.reference_num_insert() == b.reference_num_insert()
        && a.reference_base_insert() == b.reference_base_insert()
        && a.strands() == b.strands()
        && a.query_name() == b.query_name()
        && a.query_size() == b.query_size()
        && a.query_start() == b.query_start()
        && a.query_end() == b.query_end()
        && a.reference_name() == b.reference_name()
        && a.reference_size() == b.reference_size()
        && a.reference_start() == b.reference_start()
        && a.reference_end() == b.reference_end()
        && a.block_sizes() == b.block_sizes()
        && a.query_starts() == b.query_starts()
        && a.reference_starts() == b.reference_starts()
}

#[test]
fn generated_records_check_roundtrip_and_swap() {
    let mut rng = Lcg(0x1234_5678_9abc_def0);
    for case in 0..1000 {
        let text = generate(&mut rng);
        let psl = single(&text);

        // 1. The generated record is structurally valid.
        let report = check(&psl);
        assert!(
            report.is_ok(),
            "case {case}: check failed {:?}\n{text}",
            report.violations
        );

        // 2. parse -> write -> parse is the identity.
        let mut buf = Vec::new();
        psltools::write_psl(&mut buf, &psl).unwrap();
        let round = single(std::str::from_utf8(&buf).unwrap());
        assert!(
            structurally_equal(&psl, &round),
            "case {case}: round-trip differs\n{text}"
        );

        // 3. swap is an involution for canonical DNA strands.
        let twice = swap(&swap(&psl));
        assert!(
            structurally_equal(&psl, &twice),
            "case {case}: swap^2 != id\n{text}"
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn owned_record_serde_json_roundtrip() {
    use psltools::{OwnedPsl, StreamingReader};
    use std::io::{BufReader, Cursor};

    let mut rng = Lcg(0xdead_beef_cafe_0001);
    for _ in 0..100 {
        let text = generate(&mut rng);
        let mut reader = StreamingReader::new(BufReader::new(Cursor::new(text.into_bytes())));
        let record = reader.next_record().unwrap().unwrap();

        let json = serde_json::to_string(&record).unwrap();
        let back: OwnedPsl = serde_json::from_str(&json).unwrap();

        assert_eq!(record.strands, back.strands);
        assert_eq!(record.query_name, back.query_name);
        assert_eq!(record.block_sizes.as_slice(), back.block_sizes.as_slice());
        assert_eq!(record.query_starts.as_slice(), back.query_starts.as_slice());
        assert_eq!(
            record.reference_starts.as_slice(),
            back.reference_starts.as_slice()
        );
        assert_eq!(record.score(), back.score());
    }
}
