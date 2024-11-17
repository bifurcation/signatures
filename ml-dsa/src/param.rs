//! This module encapsulates all of the compile-time logic related to parameter-set dependent sizes
//! of objects.  `ParameterSet` captures the parameters in the form described by the ML-KEM
//! specification.  `EncodingSize`, `VectorEncodingSize`, and `CbdSamplingSize` are "upstream" of
//! `ParameterSet`; they provide basic logic about the size of encoded objects.  `PkeParams` and
//! `KemParams` are "downstream" of `ParameterSet`; they define derived parameters relevant to
//! K-PKE and ML-KEM.
//!
//! While the primary purpose of these traits is to describe the sizes of objects, in order to
//! avoid leakage of complicated trait bounds, they also need to provide any logic that needs to
//! know any details about object sizes.  For example, `VectorEncodingSize::flatten` needs to know
//! that the size of an encoded vector is `K` times the size of an encoded polynomial.

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Rem, Sub};

use hybrid_array::{typenum::*, Array};

use crate::algebra::{Polynomial, PolynomialVector};
use crate::encode::{BitPack, SimpleBitPack};
use crate::util::{Flatten, Unflatten, B32, B64};

/// An array length with other useful properties
pub trait ArraySize: hybrid_array::ArraySize + PartialEq + Debug {}

impl<T> ArraySize for T where T: hybrid_array::ArraySize + PartialEq + Debug {}

/// Some useful compile-time constants
pub type SpecQ = Diff<Diff<Shleft<U1, U23>, Shleft<U1, U13>>, U1>;
pub type SpecD = U13;
pub type QMinus1 = Diff<SpecQ, U1>;
pub type BitlenQMinusD = Diff<Length<SpecQ>, SpecD>;
pub type Pow2DMinus1 = Shleft<U1, Diff<SpecD, U1>>;
pub type Pow2DMinus1Minus1 = Diff<Pow2DMinus1, U1>;

/// An integer that describes a bit length to be used in sampling
pub trait SamplingSize: ArraySize + Len {
    const ETA: Eta;
}

#[derive(Copy, Clone)]
pub enum Eta {
    Two,
    Four,
}

impl SamplingSize for U2 {
    const ETA: Eta = Eta::Two;
}

impl SamplingSize for U4 {
    const ETA: Eta = Eta::Four;
}

/// An integer that can be used as a length for encoded values.
pub trait EncodingSize: ArraySize {
    type EncodedPolynomialSize: ArraySize;
    type ValueStep: ArraySize;
    type ByteStep: ArraySize;
}

type EncodingUnit<D> = Quot<Prod<D, U8>, Gcf<D, U8>>;

pub type EncodedPolynomialSize<D> = <D as EncodingSize>::EncodedPolynomialSize;
pub type EncodedPolynomial<D> = Array<u8, EncodedPolynomialSize<D>>;

impl<D> EncodingSize for D
where
    D: ArraySize + Mul<U8> + Gcd<U8> + Mul<U32>,
    Prod<D, U32>: ArraySize,
    Prod<D, U8>: Div<Gcf<D, U8>>,
    EncodingUnit<D>: Div<D> + Div<U8>,
    Quot<EncodingUnit<D>, D>: ArraySize,
    Quot<EncodingUnit<D>, U8>: ArraySize,
{
    type EncodedPolynomialSize = Prod<D, U32>;
    type ValueStep = Quot<EncodingUnit<D>, D>;
    type ByteStep = Quot<EncodingUnit<D>, U8>;
}

/// A pair of integers that describes a range
pub trait RangeEncodingSize {
    type Min: Unsigned;
    type Max: Unsigned;
    type EncodingSize: EncodingSize;
}

impl<A, B> RangeEncodingSize for (A, B)
where
    A: Unsigned + Add<B>,
    B: Unsigned,
    Sum<A, B>: Len,
    Length<Sum<A, B>>: EncodingSize,
{
    type Min = A;
    type Max = B;
    type EncodingSize = Length<Sum<A, B>>;
}

pub type RangeMin<A, B> = <(A, B) as RangeEncodingSize>::Min;
pub type RangeMax<A, B> = <(A, B) as RangeEncodingSize>::Max;
pub type RangeEncodingBits<A, B> = <(A, B) as RangeEncodingSize>::EncodingSize;
pub type RangeEncodedPolynomialSize<A, B> =
    <RangeEncodingBits<A, B> as EncodingSize>::EncodedPolynomialSize;
pub type RangeEncodedPolynomial<A, B> = Array<u8, RangeEncodedPolynomialSize<A, B>>;

/// An integer that describes a mask sampling size
pub trait MaskSamplingSize: Unsigned {
    type SampleSize: ArraySize;

    fn unpack(v: &Array<u8, Self::SampleSize>) -> Polynomial;
}

impl<G> MaskSamplingSize for G
where
    G: Unsigned + Sub<U1>,
    (Diff<G, U1>, G): RangeEncodingSize,
{
    type SampleSize = RangeEncodedPolynomialSize<Diff<G, U1>, G>;

    fn unpack(v: &Array<u8, Self::SampleSize>) -> Polynomial {
        BitPack::<Diff<G, U1>, G>::unpack(v)
    }
}

/// An integer that can describe encoded vectors.
pub trait VectorEncodingSize<K>: EncodingSize
where
    K: ArraySize,
{
    type EncodedPolynomialVectorSize: ArraySize;

    fn flatten(polys: Array<EncodedPolynomial<Self>, K>) -> EncodedPolynomialVector<Self, K>;
    fn unflatten(vec: &EncodedPolynomialVector<Self, K>) -> Array<&EncodedPolynomial<Self>, K>;
}

pub type EncodedPolynomialVectorSize<D, K> =
    <D as VectorEncodingSize<K>>::EncodedPolynomialVectorSize;
pub type EncodedPolynomialVector<D, K> = Array<u8, EncodedPolynomialVectorSize<D, K>>;

impl<D, K> VectorEncodingSize<K> for D
where
    D: EncodingSize,
    K: ArraySize,
    D::EncodedPolynomialSize: Mul<K>,
    Prod<D::EncodedPolynomialSize, K>:
        ArraySize + Div<K, Output = D::EncodedPolynomialSize> + Rem<K, Output = U0>,
{
    type EncodedPolynomialVectorSize = Prod<D::EncodedPolynomialSize, K>;

    fn flatten(polys: Array<EncodedPolynomial<Self>, K>) -> EncodedPolynomialVector<Self, K> {
        polys.flatten()
    }

    fn unflatten(vec: &EncodedPolynomialVector<Self, K>) -> Array<&EncodedPolynomial<Self>, K> {
        vec.unflatten()
    }
}

/// A `ParameterSet` captures the parameters that describe a particular instance of ML-DSA.  There
/// are three variants, corresponding to three different security levels.
pub trait ParameterSet {
    /// Number of rows in the A matrix
    type K: ArraySize;

    /// Number of columns in the A matrix
    type L: ArraySize;

    /// Private key range
    type Eta: SamplingSize;

    /// Error size bound for y
    type Gamma1: MaskSamplingSize;

    /// Low-order rounding range
    type Gamma2: Unsigned;

    /// Encoding width of the W1 polynomial, namely bitlen((q - 1) / (2 * gamma2) - 1)
    type W1Bits: EncodingSize;

    /// Collision strength of c_tilde, in bytes (lambda / 4 in the spec)
    type Lambda: ArraySize;

    /// Max number of true values in the hint
    type Omega: ArraySize;

    /// Number of nonzero values in the polynomial c
    const TAU: usize;

    /// Beta = Tau * Eta
    const BETA: u32 = (Self::TAU as u32) * Self::Eta::U32;
}

pub trait SigningKeyParams: ParameterSet {
    type S1Size: ArraySize;
    type S2Size: ArraySize;
    type T0Size: ArraySize;
    type SigningKeySize: ArraySize;

    fn encode_s1(s1: &PolynomialVector<Self::L>) -> EncodedS1<Self>;
    fn encode_s2(s2: &PolynomialVector<Self::K>) -> EncodedS2<Self>;
    fn encode_t0(t0: &PolynomialVector<Self::K>) -> EncodedT0<Self>;
    fn concat_sk(
        rho: B32,
        K: B32,
        tr: B64,
        s1: EncodedS1<Self>,
        s2: EncodedS2<Self>,
        t0: EncodedT0<Self>,
    ) -> EncodedSigningKey<Self>;
}

pub type EncodedS1<P> = Array<u8, <P as SigningKeyParams>::S1Size>;
pub type EncodedS2<P> = Array<u8, <P as SigningKeyParams>::S2Size>;
pub type EncodedT0<P> = Array<u8, <P as SigningKeyParams>::T0Size>;
pub type EncodedSigningKey<P> = Array<u8, <P as SigningKeyParams>::SigningKeySize>;

impl<P> SigningKeyParams for P
where
    P: ParameterSet,
    // General rules about Eta
    P::Eta: Add<P::Eta>,
    Sum<P::Eta, P::Eta>: Len,
    Length<Sum<P::Eta, P::Eta>>: EncodingSize,
    // S1 encoding with Eta (L-size)
    EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>: Mul<P::L>,
    Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::L>: ArraySize
        + Div<P::L, Output = EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>>
        + Rem<P::L, Output = U0>,
    // S2 encoding with Eta (K-size)
    EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>: Mul<P::K>,
    Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::K>: ArraySize
        + Div<P::K, Output = EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>>
        + Rem<P::K, Output = U0>,
    // T0 encoding in -2^{d-1}-1 .. 2^{d-1} (D bits) (416 = 32 * D)
    U416: Mul<P::K>,
    Prod<U416, P::K>: ArraySize + Div<P::K, Output = U416> + Rem<P::K, Output = U0>,
    // Signing key encoding rules
    U128: Add<Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::L>>,
    Sum<U128, Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::L>>:
        ArraySize + Add<Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::K>>,
    Sum<
        Sum<U128, Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::L>>,
        Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::K>,
    >: ArraySize + Add<Prod<U416, P::K>>,
    Sum<
        Sum<
            Sum<U128, Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::L>>,
            Prod<EncodedPolynomialSize<Length<Sum<P::Eta, P::Eta>>>, P::K>,
        >,
        Prod<U416, P::K>,
    >: ArraySize,
{
    type S1Size = EncodedPolynomialVectorSize<RangeEncodingBits<P::Eta, P::Eta>, P::L>;
    type S2Size = EncodedPolynomialVectorSize<RangeEncodingBits<P::Eta, P::Eta>, P::K>;
    type T0Size =
        EncodedPolynomialVectorSize<RangeEncodingBits<Pow2DMinus1Minus1, Pow2DMinus1>, P::K>;
    type SigningKeySize = Sum<
        Sum<
            Sum<U128, EncodedPolynomialVectorSize<RangeEncodingBits<P::Eta, P::Eta>, P::L>>,
            EncodedPolynomialVectorSize<RangeEncodingBits<P::Eta, P::Eta>, P::K>,
        >,
        EncodedPolynomialVectorSize<RangeEncodingBits<Pow2DMinus1Minus1, Pow2DMinus1>, P::K>,
    >;

    fn encode_s1(s1: &PolynomialVector<Self::L>) -> EncodedS1<Self> {
        BitPack::<P::Eta, P::Eta>::pack(s1)
    }

    fn encode_s2(s2: &PolynomialVector<Self::K>) -> EncodedS2<Self> {
        BitPack::<P::Eta, P::Eta>::pack(s2)
    }

    fn encode_t0(t0: &PolynomialVector<Self::K>) -> EncodedT0<Self> {
        BitPack::<Pow2DMinus1Minus1, Pow2DMinus1>::pack(t0)
    }

    fn concat_sk(
        rho: B32,
        K: B32,
        tr: B64,
        s1: EncodedS1<Self>,
        s2: EncodedS2<Self>,
        t0: EncodedT0<Self>,
    ) -> EncodedSigningKey<Self> {
        rho.concat(K).concat(tr).concat(s1).concat(s2).concat(t0)
    }
}

pub trait VerificationKeyParams: ParameterSet {
    type T1Size: ArraySize;
    type VerificationKeySize: ArraySize;

    fn encode_t1(t1: &PolynomialVector<Self::K>) -> EncodedT1<Self>;
    fn concat_vk(rho: B32, t1: EncodedT1<Self>) -> EncodedVerificationKey<Self>;
}

pub type EncodedT1<P> = Array<u8, <P as VerificationKeyParams>::T1Size>;
pub type EncodedVerificationKey<P> = Array<u8, <P as VerificationKeyParams>::VerificationKeySize>;

impl<P> VerificationKeyParams for P
where
    P: ParameterSet,
    // T1 encoding rules
    U320: Mul<P::K>,
    Prod<U320, P::K>: ArraySize + Div<P::K, Output = U320> + Rem<P::K, Output = U0>,
    // Verification key encoding rules
    U32: Add<Prod<U320, P::K>>,
    Sum<U32, U32>: ArraySize,
    Sum<U32, Prod<U320, P::K>>: ArraySize,
{
    type T1Size = EncodedPolynomialVectorSize<BitlenQMinusD, P::K>;
    type VerificationKeySize = Sum<U32, Self::T1Size>;

    fn encode_t1(t1: &PolynomialVector<P::K>) -> EncodedT1<Self> {
        SimpleBitPack::<BitlenQMinusD>::pack(t1)
    }

    fn concat_vk(rho: B32, t1: EncodedT1<Self>) -> EncodedVerificationKey<Self> {
        rho.concat(t1)
    }
}

pub trait SignatureParams: ParameterSet {
    type W1Size: ArraySize;
    type ZSize: ArraySize;
    type HintSize: ArraySize;
    type SignatureSize: ArraySize;

    fn encode_w1(t1: &PolynomialVector<Self::K>) -> EncodedW1<Self>;
    fn encode_z(z: &PolynomialVector<Self::L>) -> EncodedZ<Self>;

    fn concat_sig(
        c_tilde: EncodedCTilde<Self>,
        z: EncodedZ<Self>,
        h: EncodedHint<Self>,
    ) -> EncodedSignature<Self>;
}

pub type EncodedCTilde<P> = Array<u8, <P as ParameterSet>::Lambda>;
pub type EncodedW1<P> = Array<u8, <P as SignatureParams>::W1Size>;
pub type EncodedZ<P> = Array<u8, <P as SignatureParams>::ZSize>;
pub type EncodedHint<P> = Array<u8, <P as SignatureParams>::HintSize>;
pub type EncodedSignature<P> = Array<u8, <P as SignatureParams>::SignatureSize>;

impl<P> SignatureParams for P
where
    P: ParameterSet,
    // W1
    U32: Mul<P::W1Bits>,
    EncodedPolynomialSize<P::W1Bits>: Mul<P::K>,
    Prod<EncodedPolynomialSize<P::W1Bits>, P::K>:
        ArraySize + Div<P::K, Output = EncodedPolynomialSize<P::W1Bits>> + Rem<P::K, Output = U0>,
    // Z
    P::Gamma1: Sub<U1>,
    (Diff<P::Gamma1, U1>, P::Gamma1): RangeEncodingSize,
    RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>: Mul<P::L>,
    Prod<RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>, P::L>: ArraySize
        + Div<P::L, Output = RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>>
        + Rem<P::L, Output = U0>,
    // Hint
    P::Omega: Add<P::K>,
    Sum<P::Omega, P::K>: ArraySize,
    // Signature
    P::Lambda: Add<Prod<RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>, P::L>>,
    Sum<P::Lambda, Prod<RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>, P::L>>:
        ArraySize + Add<Sum<P::Omega, P::K>>,
    Sum<
        Sum<P::Lambda, Prod<RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>, P::L>>,
        Sum<P::Omega, P::K>,
    >: ArraySize,
{
    type W1Size = EncodedPolynomialVectorSize<Self::W1Bits, P::K>;
    type ZSize = Prod<RangeEncodedPolynomialSize<Diff<P::Gamma1, U1>, P::Gamma1>, P::L>;
    type HintSize = Sum<P::Omega, P::K>;
    type SignatureSize = Sum<Sum<P::Lambda, Self::ZSize>, Self::HintSize>;

    fn encode_w1(w1: &PolynomialVector<P::K>) -> EncodedW1<Self> {
        SimpleBitPack::<Self::W1Bits>::pack(w1)
    }

    fn encode_z(z: &PolynomialVector<P::L>) -> EncodedZ<Self> {
        BitPack::<Diff<P::Gamma1, U1>, P::Gamma1>::pack(z)
    }

    fn concat_sig(
        c_tilde: EncodedCTilde<Self>,
        z: EncodedZ<P>,
        h: EncodedHint<P>,
    ) -> EncodedSignature<P> {
        c_tilde.concat(z).concat(h)
    }
}
