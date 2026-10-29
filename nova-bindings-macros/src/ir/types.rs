use std::fmt;

/// A type as source-like text: `Candle<T>` rather than `Candle < T >`.
pub fn display_type(ty: &syn::Type) -> String {
    let text = zyn::ToTokens::to_token_stream(ty).to_string();
    [
        (" < ", "<"),
        (" <", "<"),
        ("< ", "<"),
        (" >", ">"),
        (" :: ", "::"),
        (":: ", "::"),
        ("& ", "&"),
        (" ,", ","),
    ]
    .iter()
    .fold(text, |text, (from, to)| text.replace(from, to))
}

/// Primitive Rust scalars that can cross a binding boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarType {
    Bool,
    F32,
    F64,
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
}

impl ScalarType {
    pub const ALL: [ScalarType; 13] = [
        ScalarType::Bool,
        ScalarType::F32,
        ScalarType::F64,
        ScalarType::I8,
        ScalarType::I16,
        ScalarType::I32,
        ScalarType::I64,
        ScalarType::Isize,
        ScalarType::U8,
        ScalarType::U16,
        ScalarType::U32,
        ScalarType::U64,
        ScalarType::Usize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ScalarType::Bool => "bool",
            ScalarType::F32 => "f32",
            ScalarType::F64 => "f64",
            ScalarType::I8 => "i8",
            ScalarType::I16 => "i16",
            ScalarType::I32 => "i32",
            ScalarType::I64 => "i64",
            ScalarType::Isize => "isize",
            ScalarType::U8 => "u8",
            ScalarType::U16 => "u16",
            ScalarType::U32 => "u32",
            ScalarType::U64 => "u64",
            ScalarType::Usize => "usize",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scalar| scalar.name() == name)
    }
}

/// Backend-neutral classification of a type in a bound signature.
#[derive(Clone, Debug)]
pub enum BindingType {
    /// `()` or no return type.
    Unit,
    Scalar(ScalarType),
    /// A generic type parameter of the indicator. Each backend instantiates
    /// it with its numeric policy type (`f64` for every current backend).
    Numeric,
    String,
    Vec(Box<BindingType>),
    Option(Box<BindingType>),
    /// `Self` or the indicator type itself; only valid as a constructor result.
    SelfType,
    /// Anything else. Kept so validation can report it with its span.
    Unsupported(Box<syn::Type>),
}

impl BindingType {
    /// `true` for the numeric types accepted by the standard `update` contract.
    pub fn is_float_scalar(&self) -> bool {
        matches!(
            self,
            BindingType::Numeric | BindingType::Scalar(ScalarType::F64)
        )
    }

    /// The first unsupported type nested in `self`, if any.
    pub fn find_unsupported(&self) -> Option<&syn::Type> {
        match self {
            BindingType::Unsupported(ty) => Some(ty),
            BindingType::Vec(inner) | BindingType::Option(inner) => inner.find_unsupported(),
            _ => None,
        }
    }

    pub fn contains_self(&self) -> bool {
        match self {
            BindingType::SelfType => true,
            BindingType::Vec(inner) | BindingType::Option(inner) => inner.contains_self(),
            _ => false,
        }
    }
}

impl fmt::Display for BindingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingType::Unit => f.write_str("()"),
            BindingType::Scalar(scalar) => f.write_str(scalar.name()),
            BindingType::Numeric => f.write_str("type parameter"),
            BindingType::String => f.write_str("String"),
            BindingType::Vec(inner) => write!(f, "Vec<{inner}>"),
            BindingType::Option(inner) => write!(f, "Option<{inner}>"),
            BindingType::SelfType => f.write_str("Self"),
            BindingType::Unsupported(ty) => f.write_str(&display_type(ty)),
        }
    }
}
