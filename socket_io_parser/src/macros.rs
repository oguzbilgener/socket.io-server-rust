/// Construct a `socket_io_parser::packet::PacketDataValue` from a JSON literal.
/// This macro is based on the serde_json::json macro.
///
///
/// ```
/// # use socket_io_parser::packet_data;
/// #
/// let value = packet_data!({
///     "code": 200,
///     "success": true,
///     "payload": {
///         "features": [
///             "serde",
///             "json"
///         ]
///     }
/// });
/// ```
///
/// Variables or expressions can be interpolated into the JSON literal. Any type
/// interpolated into an array element or object value must implement Serde's
/// `Serialize` trait, while any type interpolated into a object key must
/// implement `Into<String>`. If the `Serialize` implementation of the
/// interpolated type decides to fail, or if the interpolated type contains a
/// map with non-string keys, the `json!` macro will panic.
///
/// ```
/// # use socket_io_parser::packet_data;
/// #
/// let code = 200;
/// let features = vec!["serde", "json"];
///
/// let value = packet_data!({
///     "code": code,
///     "success": code == 200,
///     "payload": {
///         features[0]: features[1]
///     }
/// });
/// ```
///
/// Trailing commas are allowed inside both arrays and objects.
///
/// ```
/// # use socket_io_parser::packet_data;
/// #
/// let value = packet_data!([
///     "notice",
///     "the",
///     "trailing",
///     "comma -->",
/// ]);
/// ```
#[macro_export(local_inner_macros)]
macro_rules! packet_data {
    // Hide distracting implementation details from the generated rustdoc.
    ($($json:tt)+) => {
        packet_data_internal!($($json)+)
    };
}

// Rocket relies on this because they export their own `json!` with a different
// doc comment than ours, and various Rust bugs prevent them from calling our
// `json!` from their `json!` so they call `packet_data_internal!` directly. Check with
// @SergioBenitez before making breaking changes to this macro.
//
// Changes are fine as long as `packet_data_internal!` does not call any new helper
// macros and can still be invoked as `packet_data_internal!($($json)+)`.
#[macro_export(local_inner_macros)]
#[doc(hidden)]
macro_rules! packet_data_internal {
    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an array [...]. Produces a vec![...]
    // of the elements.
    //
    // Must be invoked as: packet_data_internal!(@array [] $($tt)*)
    //////////////////////////////////////////////////////////////////////////

    // Done with trailing comma.
    (@array [$($elems:expr,)*]) => {
        packet_data_internal_vec![$($elems,)*]
    };

    // Done without trailing comma.
    (@array [$($elems:expr),*]) => {
        packet_data_internal_vec![$($elems),*]
    };

    // Next element is `null`.
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!(null)] $($rest)*)
    };

    // Next element is `true`.
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!(true)] $($rest)*)
    };

    // Next element is `false`.
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!(false)] $($rest)*)
    };

    // Next element is an array.
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!([$($array)*])] $($rest)*)
    };

    // Next element is a map.
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!({$($map)*})] $($rest)*)
    };

    // Next element is an expression followed by comma.
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!($next),] $($rest)*)
    };

    // Last element is an expression with no trailing comma.
    (@array [$($elems:expr,)*] $last:expr) => {
        packet_data_internal!(@array [$($elems,)* packet_data_internal!($last)])
    };

    // Comma after the most recent element.
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        packet_data_internal!(@array [$($elems,)*] $($rest)*)
    };

    // Unexpected token after most recent element.
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        packet_data_unexpected!($unexpected)
    };

    //////////////////////////////////////////////////////////////////////////
    // TT muncher for parsing the inside of an object {...}. Each entry is
    // inserted into the given map variable.
    //
    // Must be invoked as: packet_data_internal!(@object $map () ($($tt)*) ($($tt)*))
    //
    // We require two copies of the input tokens so that we can match on one
    // copy and trigger errors on the other copy.
    //////////////////////////////////////////////////////////////////////////

    // Done.
    (@object $object:ident () () ()) => {};

    // Insert the current entry followed by trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(($($key)+).into(), $value);
        packet_data_internal!(@object $object () ($($rest)*) ($($rest)*));
    };

    // Current entry followed by unexpected token.
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        packet_data_unexpected!($unexpected);
    };

    // Insert the last entry without trailing comma.
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(($($key)+).into(), $value);
    };

    // Next value is `null`.
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!(null)) $($rest)*);
    };

    // Next value is `true`.
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!(true)) $($rest)*);
    };

    // Next value is `false`.
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!(false)) $($rest)*);
    };

    // Next value is an array.
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!([$($array)*])) $($rest)*);
    };

    // Next value is a map.
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!({$($map)*})) $($rest)*);
    };

    // Next value is an expression followed by comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!($value)) , $($rest)*);
    };

    // Last value is an expression with no trailing comma.
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        packet_data_internal!(@object $object [$($key)+] (packet_data_internal!($value)));
    };

    // Missing value for last entry. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        // "unexpected end of macro invocation"
        packet_data_internal!();
    };

    // Missing colon and value for last entry. Trigger a reasonable error
    // message.
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        // "unexpected end of macro invocation"
        packet_data_internal!();
    };

    // Misplaced colon. Trigger a reasonable error message.
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `:`".
        packet_data_unexpected!($colon);
    };

    // Found a comma inside a key. Trigger a reasonable error message.
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        // Takes no arguments so "no rules expected the token `,`".
        packet_data_unexpected!($comma);
    };

    // Key is fully parenthesized. This avoids clippy double_parens false
    // positives because the parenthesization may be necessary here.
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };

    // Refuse to absorb colon token into key expression.
    (@object $object:ident ($($key:tt)*) (: $($unexpected:tt)+) $copy:tt) => {
        packet_data_expect_expr_comma!($($unexpected)+);
    };

    // Munch a token into the current key.
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        packet_data_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };

    //////////////////////////////////////////////////////////////////////////
    // The main implementation.
    //
    // Must be invoked as: packet_data_internal!($($json)+)
    //////////////////////////////////////////////////////////////////////////

    (null) => {
        $crate::packet::PacketDataValue::Null
    };

    (true) => {
        $crate::packet::PacketDataValue::Bool(true)
    };

    (false) => {
        $crate::packet::PacketDataValue::Bool(false)
    };

    ([]) => {
        $crate::packet::PacketDataValue::Array(packet_data_internal_vec![])
    };

    ([ $($tt:tt)+ ]) => {
        $crate::packet::PacketDataValue::Array(packet_data_internal!(@array [] $($tt)+))
    };

    ({}) => {
        $crate::packet::PacketDataValue::Object($crate::Map::new())
    };

    ({ $($tt:tt)+ }) => {
        $crate::packet::PacketDataValue::Object({
            let mut object = indexmap::IndexMap::<String, PacketDataValue>::new();
            packet_data_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        })
    };

    // Any Serialize type: numbers, strings, struct literals, variables etc.
    // Must be below every other rule.
    ($other:expr) => {
        $crate::to_value(&$other).unwrap()
    };
}

// The packet_data_internal macro above cannot invoke vec directly because it uses
// local_inner_macros. A vec invocation there would resolve to $crate::vec.
// Instead invoke vec here outside of local_inner_macros.
#[macro_export]
#[doc(hidden)]
macro_rules! packet_data_internal_vec {
    ($($content:tt)*) => {
        vec![$($content)*]
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! packet_data_unexpected {
    () => {};
}

#[macro_export]
#[doc(hidden)]
macro_rules! packet_data_expect_expr_comma {
    ($e:expr , $($tt:tt)*) => {};
}

#[cfg(test)]
mod tests {
    use crate::packet::PacketDataValue;
    use serde_json::json;
    use bytes::Bytes;
    use crate::packet_data;

    // Command: RUSTFLAGS="-Z macro-backtrace" cargo +nightly test --package socket_io_parser --lib -- macros::tests::can_serialize_simple_json --exact --nocapture

    #[test]
    fn can_serialize_simple_json() {
        let left = packet_data!({
            "foo": "bar",
            "value": 42,
        });
        let right: PacketDataValue = json!({
            "foo": "bar",
            "value": 42,
        })
        .into();
        assert_eq!(left, right);
    }

    // Command: RUSTFLAGS="-Z macro-backtrace" cargo +nightly test --package socket_io_parser --lib -- macros::tests::can_serialize_nested_json --exact --nocapture

    #[test]
    fn can_serialize_nested_json() {
        let left = packet_data!({
            "foo": "bar",
            "value": 42,
            "sub": {
                "key": "value",
                "items": [1, 2, 4, 8, "yo!"],
            },
        });
        let right: PacketDataValue = json!({
            "foo": "bar",
            "value": 42,
            "sub": {
                "key": "value",
                "items": [1, 2, 4, 8, "yo!"],
            }
        })
        .into();
        assert_eq!(left, right);
    }

    // Command: RUSTFLAGS="-Z macro-backtrace" cargo +nightly test --package socket_io_parser --lib -- macros::tests::can_serialize_packet_data_with_binary_bytes --exact --nocapture

    #[test]
    fn can_serialize_packet_data_with_binary_bytes() {
        let data: Bytes = Bytes::copy_from_slice(b"abcdef!\x01\x02 q");
        let left = packet_data!({
            "foo": "bar",
            "value": 42,
            "data": data
        });

        let data: &[u8] = b"abcdef!\x01\x02 q";
        let mut map: crate::Map<String, PacketDataValue> = crate::Map::new();
        map.insert("foo".to_owned(), PacketDataValue::String("bar".to_owned()));
        map.insert("value".to_owned(), json!(42).into());
        map.insert(
            "data".to_owned(),
            PacketDataValue::Binary(bytes::Bytes::from(data).into()),
        );

        let right = PacketDataValue::Object(map);
        assert_eq!(left, right);
    }
}
