use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{
    ArrowFormat, ColorMode, ImageFormat, JsonlCompression, NumpyFormat, ParquetCompression,
    TorchSaveFormat,
};

/// Parse a parquet compression token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_parquet_compression(value: &str) -> CardPyResult<ParquetCompression> {
    match normalize_option(value).as_str() {
        "none" => Ok(ParquetCompression::None),
        "snappy" => Ok(ParquetCompression::Snappy),
        "gzip" => Ok(ParquetCompression::Gzip),
        "zstd" => Ok(ParquetCompression::Zstd),
        "lz4" => Ok(ParquetCompression::Lz4),
        got => Err(WyrdPyError::invalid_interface_option(
            "compression",
            got,
            ["none", "snappy", "gzip", "zstd", "lz4"],
        )),
    }
}

/// Parse an Arrow serialization format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_arrow_format(value: &str) -> CardPyResult<ArrowFormat> {
    match normalize_option(value).as_str() {
        "ipc" => Ok(ArrowFormat::Ipc),
        "parquet" => Ok(ArrowFormat::Parquet),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["ipc", "parquet"],
        )),
    }
}

/// Parse a `NumPy` serialization format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_numpy_format(value: &str) -> CardPyResult<NumpyFormat> {
    match normalize_option(value).as_str() {
        "npy" => Ok(NumpyFormat::Npy),
        "npz" => Ok(NumpyFormat::Npz),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["npy", "npz"],
        )),
    }
}

/// Parse a Torch save format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_torch_save_format(value: &str) -> CardPyResult<TorchSaveFormat> {
    match normalize_option(value).as_str() {
        "safetensors" => Ok(TorchSaveFormat::Safetensors),
        "pickle" => Ok(TorchSaveFormat::Pickle),
        got => Err(WyrdPyError::invalid_interface_option(
            "save_format",
            got,
            ["safetensors", "pickle"],
        )),
    }
}

/// Parse a JSON Lines compression token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_jsonl_compression(value: &str) -> CardPyResult<JsonlCompression> {
    match normalize_option(value).as_str() {
        "none" => Ok(JsonlCompression::None),
        "gzip" => Ok(JsonlCompression::Gzip),
        "zstd" => Ok(JsonlCompression::Zstd),
        got => Err(WyrdPyError::invalid_interface_option(
            "compression",
            got,
            ["none", "gzip", "zstd"],
        )),
    }
}

/// Parse an image format token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_image_format(value: &str) -> CardPyResult<ImageFormat> {
    match normalize_option(value).as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::Webp),
        "mixed" => Ok(ImageFormat::Mixed),
        got => Err(WyrdPyError::invalid_interface_option(
            "format",
            got,
            ["png", "jpeg", "webp", "mixed"],
        )),
    }
}

/// Parse an image color mode token.
///
/// # Errors
/// Returns `WYRD_DATA_400_INVALID_INTERFACE_OPTION` for unknown tokens.
pub fn parse_color_mode(value: &str) -> CardPyResult<ColorMode> {
    match normalize_option(value).as_str() {
        "rgb" => Ok(ColorMode::Rgb),
        "rgba" => Ok(ColorMode::Rgba),
        "grayscale" => Ok(ColorMode::Grayscale),
        got => Err(WyrdPyError::invalid_interface_option(
            "color_mode",
            got,
            ["rgb", "rgba", "grayscale"],
        )),
    }
}
fn normalize_option(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

pub(super) fn parquet_compression_token(value: ParquetCompression) -> &'static str {
    match value {
        ParquetCompression::None => "none",
        ParquetCompression::Snappy => "snappy",
        ParquetCompression::Gzip => "gzip",
        ParquetCompression::Zstd => "zstd",
        ParquetCompression::Lz4 => "lz4",
    }
}

pub(super) fn arrow_format_token(value: ArrowFormat) -> &'static str {
    match value {
        ArrowFormat::Ipc => "ipc",
        ArrowFormat::Parquet => "parquet",
    }
}

pub(super) fn numpy_format_token(value: NumpyFormat) -> &'static str {
    match value {
        NumpyFormat::Npy => "npy",
        NumpyFormat::Npz => "npz",
    }
}

pub(super) fn torch_save_format_token(value: TorchSaveFormat) -> &'static str {
    match value {
        TorchSaveFormat::Safetensors => "safetensors",
        TorchSaveFormat::Pickle => "pickle",
    }
}

pub(super) fn jsonl_compression_token(value: JsonlCompression) -> &'static str {
    match value {
        JsonlCompression::None => "none",
        JsonlCompression::Gzip => "gzip",
        JsonlCompression::Zstd => "zstd",
    }
}

pub(super) fn image_format_token(value: ImageFormat) -> &'static str {
    match value {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
        ImageFormat::Mixed => "mixed",
    }
}

pub(super) fn color_mode_token(value: ColorMode) -> &'static str {
    match value {
        ColorMode::Rgb => "rgb",
        ColorMode::Rgba => "rgba",
        ColorMode::Grayscale => "grayscale",
    }
}
#[cfg(test)]
mod tests {
    use super::{
        parse_arrow_format, parse_color_mode, parse_image_format, parse_jsonl_compression,
        parse_numpy_format, parse_parquet_compression, parse_torch_save_format,
    };
    use crate::error::WyrdPyError;
    use wyrd_spec::card::data::{
        ArrowFormat, ColorMode, ImageFormat, JsonlCompression, NumpyFormat, ParquetCompression,
        TorchSaveFormat,
    };

    #[test]
    fn parses_locked_interface_options() {
        assert_eq!(
            parse_parquet_compression("snappy").expect("valid compression"),
            ParquetCompression::Snappy
        );
        assert_eq!(
            parse_arrow_format("ipc").expect("valid arrow format"),
            ArrowFormat::Ipc
        );
        assert_eq!(
            parse_numpy_format("npz").expect("valid numpy format"),
            NumpyFormat::Npz
        );
        assert_eq!(
            parse_torch_save_format("pickle").expect("valid torch format"),
            TorchSaveFormat::Pickle
        );
        assert_eq!(
            parse_jsonl_compression("zstd").expect("valid jsonl compression"),
            JsonlCompression::Zstd
        );
        assert_eq!(
            parse_image_format("webp").expect("valid image format"),
            ImageFormat::Webp
        );
        assert_eq!(
            parse_color_mode("rgba").expect("valid color mode"),
            ColorMode::Rgba
        );
    }

    #[test]
    fn rejects_unknown_interface_options_with_wyrd_code() {
        assert_invalid_interface_option(
            parse_parquet_compression("brotli").expect_err("invalid compression"),
        );
        assert_invalid_interface_option(parse_arrow_format("feather").expect_err("invalid format"));
        assert_invalid_interface_option(parse_numpy_format("txt").expect_err("invalid format"));
        assert_invalid_interface_option(
            parse_torch_save_format("unsafe_pickle").expect_err("invalid format"),
        );
        assert_invalid_interface_option(
            parse_jsonl_compression("zip").expect_err("invalid compression"),
        );
        assert_invalid_interface_option(parse_image_format("tiff").expect_err("invalid format"));
        assert_invalid_interface_option(parse_color_mode("cmyk").expect_err("invalid color mode"));
    }

    fn assert_invalid_interface_option(error: WyrdPyError) {
        match error {
            WyrdPyError::Spec(error) => {
                assert_eq!(error.code(), "WYRD_DATA_400_INVALID_INTERFACE_OPTION");
            }
            other => panic!("expected Wyrd spec error, got {other:?}"),
        }
    }
}
