use super::*;

// Independently generated with Python struct/zlib; fixes format-1 byte compatibility.
const GOLDEN: &[u8] = &[
    0x52, 0x41, 0x59, 0x53, 0x41, 0x56, 0x45, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00,
    0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x6e, 0x82, 0x40, 0x61, 0x6c, 0x70, 0x68,
    0x61,
];

#[test]
fn golden_format_round_trip_borrows_payload_and_checks_game_schema_explicitly() {
    let encoded = encode(7, b"alpha", SaveLimits::default()).unwrap();
    assert_eq!(encoded, GOLDEN);
    let decoded = decode(&encoded, SaveLimits::default()).unwrap();
    assert_eq!(
        decoded,
        SaveRef {
            schema_version: 7,
            payload: b"alpha"
        }
    );
    assert_eq!(decoded.payload.as_ptr(), encoded[HEADER_LEN..].as_ptr());
    decoded.require_schema(7).unwrap();
    assert!(matches!(
        decoded.require_schema(8),
        Err(SaveError::UnsupportedSchema {
            expected: 8,
            found: 7
        })
    ));
}

#[test]
fn reusable_encoding_preserves_bytes_on_rejection_and_allows_empty_payloads() {
    let mut output = Vec::with_capacity(1024);
    let ptr = output.as_ptr();
    encode_into(
        &mut output,
        7,
        b"alpha",
        SaveLimits {
            max_payload_bytes: 5,
        },
    )
    .unwrap();
    assert_eq!(output, GOLDEN);
    assert_eq!(ptr, output.as_ptr());
    assert!(matches!(
        encode_into(
            &mut output,
            7,
            b"alpha",
            SaveLimits {
                max_payload_bytes: 4
            }
        ),
        Err(SaveError::TooLarge { bytes: 5, limit: 4 })
    ));
    assert_eq!(output, GOLDEN);
    assert!(matches!(
        decode(
            GOLDEN,
            SaveLimits {
                max_payload_bytes: 4
            }
        ),
        Err(SaveError::TooLarge { .. })
    ));
    encode_into(
        &mut output,
        0,
        &[],
        SaveLimits {
            max_payload_bytes: 0,
        },
    )
    .unwrap();
    assert_eq!(output.len(), HEADER_LEN);
    assert_eq!(
        decode(
            &output,
            SaveLimits {
                max_payload_bytes: 0
            }
        )
        .unwrap(),
        SaveRef {
            schema_version: 0,
            payload: &[]
        }
    );
}

#[test]
fn malformed_truncated_extended_and_unsupported_containers_are_rejected() {
    for length in 0..HEADER_LEN {
        assert!(
            matches!(decode(&GOLDEN[..length], SaveLimits::default()), Err(SaveError::TruncatedHeader { bytes }) if bytes == length)
        );
    }
    let mut data = GOLDEN.to_vec();
    data[0] ^= 1;
    assert!(matches!(
        decode(&data, SaveLimits::default()),
        Err(SaveError::BadMagic)
    ));
    data = GOLDEN.to_vec();
    data[8..12].copy_from_slice(&9_u32.to_le_bytes());
    assert!(matches!(
        decode(&data, SaveLimits::default()),
        Err(SaveError::UnsupportedFormat { found: 9 })
    ));
    for index in [12, 24, HEADER_LEN, GOLDEN.len() - 1] {
        data = GOLDEN.to_vec();
        data[index] ^= 1;
        assert!(matches!(
            decode(&data, SaveLimits::default()),
            Err(SaveError::ChecksumMismatch { .. })
        ));
    }
    data = GOLDEN.to_vec();
    data.pop();
    assert!(matches!(
        decode(&data, SaveLimits::default()),
        Err(SaveError::LengthMismatch {
            declared: 5,
            actual: 4
        })
    ));
    data = GOLDEN.to_vec();
    data.push(0);
    assert!(matches!(
        decode(&data, SaveLimits::default()),
        Err(SaveError::LengthMismatch {
            declared: 5,
            actual: 6
        })
    ));
    data[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        decode(&data, SaveLimits::default()),
        Err(SaveError::TooLarge { .. })
    ));
    assert!(matches!(
        decode(
            &data,
            SaveLimits {
                max_payload_bytes: usize::MAX
            }
        ),
        Err(SaveError::SizeOverflow | SaveError::TooLarge { .. })
    ));
}
