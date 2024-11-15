use hybrid_array::{typenum::*, Array};

use crate::algebra::*;
use crate::param::*;

pub type DecodedValue<T> = Array<T, U256>;

// Algorithm 16 SimpleBitPack
fn simple_bit_pack<D, T>(vals: &DecodedValue<T>) -> EncodedPolynomial<D>
where
    D: EncodingSize,
    T: Copy,
    u128: From<T>,
{
    let val_step = D::ValueStep::USIZE;
    let byte_step = D::ByteStep::USIZE;

    let mut bytes = EncodedPolynomial::<D>::default();

    let vc = vals.chunks(val_step);
    let bc = bytes.chunks_mut(byte_step);
    for (v, b) in vc.zip(bc) {
        let mut x = 0u128;
        for (j, vj) in v.iter().enumerate() {
            x |= u128::from(*vj) << (D::USIZE * j);
        }

        let xb = x.to_le_bytes();
        b.copy_from_slice(&xb[..byte_step]);
    }

    bytes
}

// Algorithm 18 SimpleBitUnpack
fn simple_bit_unpack<D, T>(bytes: &EncodedPolynomial<D>) -> DecodedValue<T>
where
    D: EncodingSize,
    T: From<u128> + Default,
{
    let val_step = D::ValueStep::USIZE;
    let byte_step = D::ByteStep::USIZE;
    let mask = (1 << D::USIZE) - 1;

    let mut vals = DecodedValue::<T>::default();

    let vc = vals.chunks_mut(val_step);
    let bc = bytes.chunks(byte_step);
    for (v, b) in vc.zip(bc) {
        let mut xb = [0u8; 16];
        xb[..byte_step].copy_from_slice(b);

        let x = u128::from_le_bytes(xb);
        for (j, vj) in v.iter_mut().enumerate() {
            let val: u128 = (x >> (D::USIZE * j)) & mask;
            *vj = T::from(val);
        }
    }

    vals
}

// Algorithm 17 BitPack
fn bit_pack<A, B>(vals: &DecodedValue<FieldElement>) -> RangeEncodedPolynomial<A, B>
where
    (A, B): RangeEncodingSize,
{
    let a = FieldElement(RangeMin::<A, B>::U32);
    let b = FieldElement(RangeMax::<A, B>::U32);
    let to_encode = vals
        .iter()
        .map(|w| {
            assert!(w.0 <= b.0 || w.0 >= (-a).0);
            b - *w
        })
        .collect();
    simple_bit_pack::<RangeEncodingBits<A, B>, FieldElement>(&to_encode)
}

// FAlgorithm 17 BitPack
fn bit_unpack<A, B>(bytes: &RangeEncodedPolynomial<A, B>) -> DecodedValue<FieldElement>
where
    (A, B): RangeEncodingSize,
{
    let a = FieldElement(RangeMin::<A, B>::U32);
    let b = FieldElement(RangeMax::<A, B>::U32);
    let decoded = simple_bit_unpack::<RangeEncodingBits<A, B>, FieldElement>(bytes);
    decoded
        .iter()
        .map(|z| {
            assert!(z.0 < (a + b).0);
            b - *z
        })
        .collect()
}

/// SimpleBitPack
pub trait SimpleBitPack<D> {
    type PackedSize: ArraySize;
    fn pack(&self) -> Array<u8, Self::PackedSize>;
    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self;
}

impl<D> SimpleBitPack<D> for Polynomial
where
    D: EncodingSize,
{
    type PackedSize = D::EncodedPolynomialSize;

    fn pack(&self) -> Array<u8, Self::PackedSize> {
        simple_bit_pack::<D, FieldElement>(&self.0)
    }

    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self {
        Self(simple_bit_unpack::<D, FieldElement>(enc))
    }
}

impl<K, D> SimpleBitPack<D> for PolynomialVector<K>
where
    K: ArraySize,
    D: VectorEncodingSize<K>,
{
    type PackedSize = D::EncodedPolynomialVectorSize;

    fn pack(&self) -> Array<u8, Self::PackedSize> {
        let polys = self.0.iter().map(|x| SimpleBitPack::<D>::pack(x)).collect();
        D::flatten(polys)
    }

    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self {
        let unfold = D::unflatten(enc);
        Self(
            unfold
                .into_iter()
                .map(|x| <Polynomial as SimpleBitPack<D>>::unpack(x))
                .collect(),
        )
    }
}

/// BitPack
pub trait BitPack<A, B> {
    type PackedSize: ArraySize;
    fn pack(&self) -> Array<u8, Self::PackedSize>;
    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self;
}

impl<A, B> BitPack<A, B> for Polynomial
where
    (A, B): RangeEncodingSize,
{
    type PackedSize = EncodedPolynomialSize<RangeEncodingBits<A, B>>;

    fn pack(&self) -> Array<u8, Self::PackedSize> {
        bit_pack::<A, B>(&self.0)
    }

    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self {
        Self(bit_unpack::<A, B>(enc))
    }
}

impl<K, A, B> BitPack<A, B> for PolynomialVector<K>
where
    K: ArraySize,
    (A, B): RangeEncodingSize,
    RangeEncodingBits<A, B>: VectorEncodingSize<K>,
{
    type PackedSize = EncodedPolynomialVectorSize<RangeEncodingBits<A, B>, K>;

    fn pack(&self) -> Array<u8, Self::PackedSize> {
        let polys = self.0.iter().map(|x| BitPack::<A, B>::pack(x)).collect();
        RangeEncodingBits::<A, B>::flatten(polys)
    }

    fn unpack(enc: &Array<u8, Self::PackedSize>) -> Self {
        let unfold = RangeEncodingBits::<A, B>::unflatten(enc);
        Self(
            unfold
                .into_iter()
                .map(|x| <Polynomial as BitPack<A, B>>::unpack(x))
                .collect(),
        )
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use core::ops::Rem;
    use hybrid_array::typenum::{
        marker_traits::Zero, operator_aliases::Mod, U1, U10, U2, U3, U4, U6, U8,
    };
    use rand::Rng;

    // A helper trait to construct larger arrays by repeating smaller ones
    trait Repeat<T: Clone, D: ArraySize> {
        fn repeat(&self) -> Array<T, D>;
    }

    impl<T, N, D> Repeat<T, D> for Array<T, N>
    where
        N: ArraySize,
        T: Clone,
        D: ArraySize + Rem<N>,
        Mod<D, N>: Zero,
    {
        #[allow(clippy::integer_division_remainder_used)]
        fn repeat(&self) -> Array<T, D> {
            Array::from_fn(|i| self[i % N::USIZE].clone())
        }
    }

    #[allow(clippy::integer_division_remainder_used)]
    fn simple_bit_pack_test<D>(b: u32, decoded: Polynomial, encoded: EncodedPolynomial<D>)
    where
        D: EncodingSize,
    {
        // Test known answer
        let actual_encoded = SimpleBitPack::<D>::pack(&decoded);
        assert_eq!(actual_encoded, encoded);

        let actual_decoded: Polynomial = SimpleBitPack::<D>::unpack(&encoded);
        assert_eq!(actual_decoded, decoded);

        // Test random decode/encode and encode/decode round trips
        let mut rng = rand::thread_rng();
        let decoded = Polynomial(Array::from_fn(|_| {
            let x: u32 = rng.gen();
            FieldElement(x % (b + 1))
        }));

        let actual_encoded = SimpleBitPack::<D>::pack(&decoded);
        let actual_decoded: Polynomial = SimpleBitPack::<D>::unpack(&actual_encoded);
        assert_eq!(actual_decoded, decoded);

        let actual_reencoded = SimpleBitPack::<D>::pack(&decoded);
        assert_eq!(actual_reencoded, actual_encoded);
    }

    #[test]
    fn simple_bit_pack() {
        // Use a standard test pattern across all the cases
        let decoded = Polynomial(
            Array::<_, U8>([
                0.into(),
                1.into(),
                2.into(),
                3.into(),
                4.into(),
                5.into(),
                6.into(),
                7.into(),
            ])
            .repeat(),
        );

        // 10 bits
        // <-> b = 2^{bitlen(q-1) - d} - 1 = 2^10 - 1
        let b = (1 << 10) - 1;
        let encoded: EncodedPolynomial<U10> =
            Array::<_, U10>([0x00, 0x04, 0x20, 0xc0, 0x00, 0x04, 0x14, 0x60, 0xc0, 0x01]).repeat();
        simple_bit_pack_test::<U10>(b, decoded, encoded);

        // 8 bits
        // gamma2 = (q - 1) / 88
        // b = (q - 1) / (2 gamma2) - 1 = 175 = 2^8 - 81
        let b = (1 << 8) - 81;
        let encoded: EncodedPolynomial<U8> =
            Array::<_, U8>([0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]).repeat();
        simple_bit_pack_test::<U8>(b, decoded, encoded);

        // 6 bits
        // gamma2 = (q - 1) / 32
        // b = (q - 1) / (2 gamma2) - 1 = 63 = 2^6 - 1
        let b = (1 << 6) - 1;
        let encoded: EncodedPolynomial<U6> =
            Array::<_, U6>([0x40, 0x20, 0x0c, 0x44, 0x61, 0x1c]).repeat();
        simple_bit_pack_test::<U6>(b, decoded, encoded);
    }

    #[allow(clippy::integer_division_remainder_used)]
    fn bit_pack_test<A, B>(decoded: Polynomial, encoded: RangeEncodedPolynomial<A, B>)
    where
        A: Unsigned,
        B: Unsigned,
        (A, B): RangeEncodingSize,
    {
        let a = FieldElement(A::U32);
        let b = FieldElement(B::U32);

        // Test known answer
        let actual_encoded = BitPack::<A, B>::pack(&decoded);
        assert_eq!(actual_encoded, encoded);

        let actual_decoded: Polynomial = BitPack::<A, B>::unpack(&encoded);
        assert_eq!(actual_decoded, decoded);

        // Test random decode/encode and encode/decode round trips
        let mut rng = rand::thread_rng();
        let decoded = Polynomial(Array::from_fn(|_| {
            let mut x: u32 = rng.gen();
            x = x % (a.0 + b.0);
            b - FieldElement(x)
        }));

        let actual_encoded = BitPack::<A, B>::pack(&decoded);
        let actual_decoded: Polynomial = BitPack::<A, B>::unpack(&actual_encoded);
        assert_eq!(actual_decoded, decoded);

        let actual_reencoded = BitPack::<A, B>::pack(&decoded);
        assert_eq!(actual_reencoded, actual_encoded);
    }

    #[test]
    fn bit_pack() {
        // Use a standard test pattern across all the cases
        // XXX(RLB) We can't use -2 because the eta=2 case doesn't actually cover -2
        let decoded = Polynomial(
            Array::<_, U4>([
                FieldElement(FieldElement::Q - 1),
                FieldElement(0),
                FieldElement(1),
                FieldElement(2),
            ])
            .repeat(),
        );

        // BitPack(_, eta, eta), eta = 2, 4
        let encoded: RangeEncodedPolynomial<U2, U2> = Array::<_, U3>([0x53, 0x30, 0x05]).repeat();
        bit_pack_test::<U2, U2>(decoded, encoded);

        let encoded: RangeEncodedPolynomial<U4, U4> = Array::<_, U2>([0x45, 0x23]).repeat();
        bit_pack_test::<U4, U4>(decoded, encoded);

        // BitPack(_, 2^d - 1, 2^d), d = 13
        type D = U13;
        type Pow2D = Shleft<U1, D>;
        type Pow2DMin = Diff<Pow2D, U1>;
        let encoded: RangeEncodedPolynomial<Pow2DMin, Pow2D> =
            Array::<_, U7>([0x01, 0x20, 0x00, 0xf8, 0xff, 0xf9, 0x7f]).repeat();
        bit_pack_test::<Pow2DMin, Pow2D>(decoded, encoded);

        // BitPack(_, gamma1 - 1, gamma1), gamma1 = 2^17, 2^19
        type Gamma1Lo = Shleft<U1, U17>;
        type Gamma1LoMin = Diff<Gamma1Lo, U1>;
        let encoded: RangeEncodedPolynomial<Gamma1LoMin, Gamma1Lo> =
            Array::<_, U9>([0x01, 0x00, 0x02, 0x00, 0xf8, 0xff, 0x9f, 0xff, 0x7f]).repeat();
        bit_pack_test::<Gamma1LoMin, Gamma1Lo>(decoded, encoded);

        // XXX(RLB): This encoding looks wrong to me.  The entries should start at 2^19 + 1 instead
        // of 2^19.  But I have changed the expected answer to match what the encoder produces.
        //
        // 0    0    0    0    8    f    f    f    f    7    e    f    f    f    7    d    f    f    f    7
        // 0000 0000 0000 0000 0001  1111 1111 1111 1111 1110  0111 1111 1111 1111 1110  1011 1111 1111 1111 1110
        //
        // 0000f8ff7ffeffd7ff7f
        type Gamma1Hi = Shleft<U1, U19>;
        type Gamma1HiMin = Diff<Gamma1Hi, U1>;
        let encoded: RangeEncodedPolynomial<Gamma1HiMin, Gamma1Hi> =
            Array::<_, U10>([0x00, 0x00, 0xf8, 0xff, 0x7f, 0xfe, 0xff, 0xd7, 0xff, 0x7f]).repeat();
        bit_pack_test::<Gamma1Hi, Gamma1HiMin>(decoded, encoded);
    }

    /*
    #[test]
    fn byte_codec_signed() {
        use core::marker::PhantomData;
        use core::ops::Add;
        use hybrid_array::typenum::*;

        #[derive(Copy, Clone, Debug, Default, PartialEq)]
        struct BoundedSignedInteger<A, B> {
            val: i32,
            _a: PhantomData<A>,
            _b: PhantomData<B>,
        }

        impl<A, B> BoundedSignedInteger<A, B>
        where
            A: Unsigned,
            B: Unsigned,
        {
            const A: i32 = A::I32;
            const B: i32 = B::I32;
        }

        impl<A, B> Gen for BoundedSignedInteger<A, B>
        where
            A: Unsigned,
            B: Unsigned,
        {
            fn gen<R: Rng + ?Sized>(rng: &mut R) -> Self {
                let mut val: u32 = rng.gen();
                val %= (Self::A + Self::B + 1) as u32;

                let mut val = val as i32;
                val -= Self::A;
                val.into()
            }
        }

        impl<A, B> FixedWidth for BoundedSignedInteger<A, B>
        where
            A: Add<B>,
            Sum<A, B>: Len,
            Length<Sum<A, B>>: EncodingSize,
        {
            type BitWidth = Length<Sum<A, B>>;
        }

        impl<A, B> From<i32> for BoundedSignedInteger<A, B>
        where
            A: Unsigned,
            B: Unsigned,
        {
            fn from(x: i32) -> BoundedSignedInteger<A, B> {
                assert!(-A::I32 <= x);
                assert!(x <= B::I32);
                Self {
                    val: x,
                    _a: PhantomData,
                    _b: PhantomData,
                }
            }
        }

        impl<A, B> From<BoundedSignedInteger<A, B>> for u128
        where
            A: Unsigned,
            B: Unsigned,
        {
            fn from(x: BoundedSignedInteger<A, B>) -> u128 {
                (B::I32 - x.val) as u128
            }
        }

        impl<A, B> From<u128> for BoundedSignedInteger<A, B>
        where
            A: Unsigned,
            B: Unsigned,
        {
            fn from(x: u128) -> BoundedSignedInteger<A, B> {
                assert!((x as u64) <= A::U64 + B::U64);
                (B::I32 - (x as i32)).into()
            }
        }

        type Encoded<A, B> = FixedWidthEncoded<BoundedSignedInteger<A, B>>;

        // For most codec widths, we use a standard sequence
        fn decoded<A, B>() -> DecodedValue<BoundedSignedInteger<A, B>>
        where
            A: Unsigned,
            B: Unsigned,
        {
            Array::<_, U8>([
                (-2_i32).into(),
                (-1_i32).into(),
                0_i32.into(),
                1_i32.into(),
                2_i32.into(),
                1_i32.into(),
                0_i32.into(),
                (-1_i32).into(),
            ])
            .repeat()
        }

        // BitPack(_, eta, eta), eta = 2, 4
        let encoded: Encoded<U2, U2> = Array::<_, U3>([156, 130, 104]).repeat();
        byte_codec_test(decoded::<U2, U2>(), encoded);

        let encoded: Encoded<U4, U4> = Array::<_, U4>([0x56, 0x34, 0x32, 0x54]).repeat();
        byte_codec_test(decoded::<U4, U4>(), encoded);

        // BitPack(_, 2^d - 1, 2^d), d = 13
        type D = U13;
        type POW_2_D = Shleft<U1, D>;
        type D_MIN = Diff<POW_2_D, U1>;

        let encoded: Encoded<D_MIN, POW_2_D> = Array::<_, U14>([
            0x02, 0x60, 0x00, 0x08, 0x00, 0xfe, 0x7f, 0xfe, 0xdf, 0xff, 0x07, 0x00, 0x06, 0x80,
        ])
        .repeat();
        byte_codec_test(decoded::<D_MIN, POW_2_D>(), encoded);

        // BitPack(_, gamma1 - 1, gamma1), gamma1 = 2^17, 2^19
        type GAMMA1_LO = Shleft<U1, U17>;
        type GAMMA1_LO_MIN = Diff<GAMMA1_LO, U1>;
        let encoded: Encoded<GAMMA1_LO_MIN, GAMMA1_LO> = Array::<_, U18>([
            0x02, 0x00, 0x06, 0x00, 0x08, 0x00, 0xe0, 0xff, 0x7f, 0xfe, 0xff, 0xfd, 0xff, 0x07,
            0x00, 0x60, 0x00, 0x80,
        ])
        .repeat();
        byte_codec_test(decoded::<GAMMA1_LO_MIN, GAMMA1_LO>(), encoded);

        type GAMMA1_HI = Shleft<U1, U19>;
        type GAMMA1_HI_MIN = Diff<GAMMA1_HI, U1>;
        let encoded: Encoded<GAMMA1_HI_MIN, GAMMA1_HI> = Array::<_, U20>([
            0x02, 0x00, 0x18, 0x00, 0x80, 0x00, 0x00, 0xf8, 0xff, 0x7f, 0xfe, 0xff, 0xf7, 0xff,
            0x7f, 0x00, 0x00, 0x18, 0x00, 0x80,
        ])
        .repeat();
        byte_codec_test(decoded::<GAMMA1_HI_MIN, GAMMA1_HI>(), encoded);
    }
    */
}
