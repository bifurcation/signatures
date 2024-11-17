use hybrid_array::{
    typenum::{Unsigned, U256},
    Array,
};

use crate::algebra::*;
use crate::param::*;

fn make_hint<Gamma2: Unsigned>(z: FieldElement, r: FieldElement) -> bool {
    // XXX(RLB): Maybe propagate the Gamma2 into these methods
    let r1 = r.high_bits::<Gamma2>();
    let v1 = (r + z).high_bits::<Gamma2>();
    r1 != v1
}

fn use_hint<Gamma2: Unsigned>(h: bool, r: FieldElement) -> FieldElement {
    // XXX(RLB) Can we make this const?
    let m: u32 = (FieldElement::Q - 1) / (2 * Gamma2::U32);
    let (r1, r0) = r.decompose::<Gamma2>();
    if h && r0.0 <= Gamma2::U32 {
        FieldElement((r1.0 + 1) % m)
    } else if h && r0.0 > FieldElement::Q - Gamma2::U32 {
        FieldElement((r1.0 + m - 1) % m)
    } else if h {
        // We use the FieldElement encoding even for signed integers.  Since r0 is computed
        // mod+- 2*gamma2, it is guaranteed to be in (gamma2, gamma2].
        unreachable!();
    } else {
        r1
    }
}

pub struct Hint<P>(Array<Array<bool, U256>, P::K>)
where
    P: SignatureParams;

impl<P> Hint<P>
where
    P: SignatureParams,
{
    pub fn new(z: PolynomialVector<P::K>, r: PolynomialVector<P::K>) -> Self {
        let zi = z.0.iter();
        let ri = r.0.iter();

        Self(
            zi.zip(ri)
                .map(|(zv, rv)| {
                    let zvi = zv.0.iter();
                    let rvi = rv.0.iter();

                    zvi.zip(rvi)
                        .map(|(&z, &r)| make_hint::<P::Gamma2>(z, r))
                        .collect()
                })
                .collect(),
        )
    }

    pub fn hamming_weight(&self) -> usize {
        self.0
            .iter()
            .map(|x| x.iter().filter(|x| **x).count())
            .sum()
    }

    pub fn use_hint(&self, r: &PolynomialVector<P::K>) -> PolynomialVector<P::K> {
        let hi = self.0.iter();
        let ri = r.0.iter();

        PolynomialVector(
            hi.zip(ri)
                .map(|(hv, rv)| {
                    let hvi = hv.iter();
                    let rvi = rv.0.iter();

                    Polynomial(
                        hvi.zip(rvi)
                            .map(|(&h, &r)| use_hint::<P::Gamma2>(h, r))
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}
