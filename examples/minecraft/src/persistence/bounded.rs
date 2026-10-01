//! Sequence admission before deserialization can grow beyond game budgets.
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct Bounded<T, const N: usize>(pub Vec<T>);
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Bounded<T, N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct List<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for List<T, N> {
            type Value = Bounded<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "at most {N} entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.size_hint().is_some_and(|n| n > N) {
                    return Err(de::Error::custom("sequence exceeds save budget"));
                }
                let mut entries = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(N));
                while let Some(entry) = seq.next_element()? {
                    if entries.len() == N {
                        return Err(de::Error::custom("sequence exceeds save budget"));
                    }
                    if entries.len() == entries.capacity() {
                        let capacity = entries.capacity().saturating_mul(2).max(1).min(N);
                        entries
                            .try_reserve_exact(capacity - entries.len())
                            .map_err(de::Error::custom)?;
                    }
                    entries.push(entry);
                }
                Ok(Bounded(entries))
            }
        }
        deserializer.deserialize_seq(List::<T, N>(PhantomData))
    }
}
