//! Local artifact layout conventions for data interface metadata.

use std::path::{Path, PathBuf};

use wyrd_spec::card::data::{ArrowFormat, DataInterface, NumpyFormat, TorchSaveFormat};

use crate::data::io::jsonl_relative_path_for;
use crate::error::CardPyResult;

/// Local artifact path capability for reconstructable Rust interface metadata.
pub trait LocalArtifactLayout {
    /// Return the convention path for this interface under a card-local root.
    ///
    /// # Errors
    /// Returns an error only when a concrete implementation cannot derive a
    /// valid path from its metadata.
    fn artifact_path(&self, base: &Path) -> CardPyResult<PathBuf>;
}

impl LocalArtifactLayout for DataInterface {
    fn artifact_path(&self, base: &Path) -> CardPyResult<PathBuf> {
        let relative = match self {
            Self::Pandas(_) | Self::Polars(_) | Self::Parquet(_) => {
                PathBuf::from("data/data.parquet")
            }
            Self::Arrow(meta) => match meta.format {
                ArrowFormat::Ipc => PathBuf::from("data/data.arrow"),
                ArrowFormat::Parquet => PathBuf::from("data/data.parquet"),
            },
            Self::Numpy(meta) => match meta.format {
                NumpyFormat::Npy => PathBuf::from("data/data.npy"),
                NumpyFormat::Npz => PathBuf::from("data/data.npz"),
            },
            Self::Torch(meta) => match meta.save_format {
                TorchSaveFormat::Safetensors => PathBuf::from("data/data.safetensors"),
                TorchSaveFormat::Pickle => PathBuf::from("data/data.pt"),
            },
            Self::Sql(_) => PathBuf::from("data/sql.json"),
            Self::Jsonl(meta) => jsonl_relative_path_for(meta.compression),
            Self::Image(_) | Self::Text(_) => PathBuf::from("data/manifest.json"),
            Self::Huggingface(meta) if meta.revision.is_some() => {
                PathBuf::from("data/dataset_pointer.json")
            }
            Self::Huggingface(_) => PathBuf::from("data/dataset"),
            Self::Custom(_) => PathBuf::from("data/custom"),
        };
        Ok(base.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::LocalArtifactLayout;
    use wyrd_spec::card::data::{
        ColorMode, CustomDataMeta, DataInterface, HuggingfaceMeta, ImageFormat, ImageMeta,
        JsonlCompression, JsonlMeta, PandasMeta, ParquetCompression,
    };

    #[test]
    fn artifact_path_matches_core_conventions() {
        let base = std::path::Path::new("/tmp/card");
        let pandas = DataInterface::Pandas(PandasMeta {
            framework_version: "2.0.0".to_string(),
            compression: ParquetCompression::Snappy,
        });
        let jsonl = DataInterface::Jsonl(JsonlMeta {
            compression: JsonlCompression::Gzip,
            lines_per_file: None,
        });
        let image = DataInterface::Image(ImageMeta {
            format: ImageFormat::Mixed,
            manifest_ref: None,
            color_mode: ColorMode::Rgb,
        });
        let custom = DataInterface::Custom(CustomDataMeta {
            loader_module: "module".to_string(),
            loader_class: "Loader".to_string(),
            extra: std::collections::BTreeMap::new(),
        });

        assert_eq!(
            pandas
                .artifact_path(base)
                .expect("pandas path should resolve"),
            base.join("data/data.parquet")
        );
        assert_eq!(
            jsonl
                .artifact_path(base)
                .expect("jsonl path should resolve"),
            base.join("data/data.jsonl.gz")
        );
        assert_eq!(
            image
                .artifact_path(base)
                .expect("image path should resolve"),
            base.join("data/manifest.json")
        );
        assert_eq!(
            custom
                .artifact_path(base)
                .expect("custom path should resolve"),
            base.join("data/custom")
        );
    }

    #[test]
    fn huggingface_artifact_path_tracks_pointer_vs_local_dataset() {
        let base = std::path::Path::new("/tmp/card");
        let pointer = DataInterface::Huggingface(HuggingfaceMeta {
            dataset_id: "namespace/dataset".to_string(),
            revision: Some("abcdef1".to_string()),
            split: Some("train".to_string()),
            config: None,
        });
        let local = DataInterface::Huggingface(HuggingfaceMeta {
            dataset_id: "namespace/dataset".to_string(),
            revision: None,
            split: None,
            config: None,
        });

        assert_eq!(
            pointer
                .artifact_path(base)
                .expect("pointer path should resolve"),
            base.join("data/dataset_pointer.json")
        );
        assert_eq!(
            local
                .artifact_path(base)
                .expect("local path should resolve"),
            base.join("data/dataset")
        );
    }
}
