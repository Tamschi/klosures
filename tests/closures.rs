#![allow(clippy::unnecessary_fold)]

use klosures::klosures;

#[klosures]
pub fn klosures_test() {
	let a = vec![(1,), (2,), (3,), (4,), (5,)];
	let average = a.iter().map((it.0)).fold(0, (it0 + it1)) / a.len();
	dbg!(average);
}
