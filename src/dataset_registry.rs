pub const REGISTRY_FILE_NAME: &str = "registry.json";

use crate::table_dataset::TableEntry;

pub struct DatasetRegistryEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub domain: &'static str,
    pub access: &'static str,
    pub status: &'static str,
    pub source_url: &'static str,
    pub connector: &'static str,
    pub notes: &'static str,
}

pub fn official_dataset_registry_json() -> String {
    let datasets: Vec<serde_json::Value> = official_dataset_registry()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "domain": entry.domain,
                "access": entry.access,
                "status": entry.status,
                "source_url": entry.source_url,
                "connector": entry.connector,
                "notes": entry.notes,
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({ "datasets": datasets })).unwrap()
}

pub fn official_dataset_table_entries() -> Vec<TableEntry> {
    official_dataset_registry()
        .into_iter()
        .map(|entry| {
            let metadata_json = serde_json::to_string_pretty(&serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "domain": entry.domain,
                "access": entry.access,
                "status": entry.status,
                "source_url": entry.source_url,
                "connector": entry.connector,
                "notes": entry.notes,
            }))
            .unwrap();

            TableEntry {
                id: entry.id.to_string(),
                metadata_json,
            }
        })
        .collect()
}

pub fn official_dataset_registry() -> Vec<DatasetRegistryEntry> {
    vec![
        DatasetRegistryEntry {
            id: "cameo",
            name: "CAMEO",
            domain: "protein_sciences",
            access: "public_web",
            status: "planned",
            source_url: "https://cameo3d.org",
            connector: "metadata",
            notes: "Continuous protein structure prediction assessment; expose target and assessment metadata first.",
        },
        DatasetRegistryEntry {
            id: "msa_databases",
            name: "MSA databases for AlphaFold/OpenFold",
            domain: "protein_sciences",
            access: "bulk_bundle",
            status: "planned",
            source_url: "https://github.com/aqlaboratory/openfold/blob/main/scripts/download_alphafold_dbs.sh",
            connector: "bundle",
            notes: "Large inference-support database bundle; represent component databases and local install paths instead of downloading by default.",
        },
        DatasetRegistryEntry {
            id: "alphafold_db",
            name: "AlphaFold DB",
            domain: "protein_sciences",
            access: "public_api",
            status: "planned",
            source_url: "https://alphafold.ebi.ac.uk/api-docs",
            connector: "api",
            notes: "API and bulk download access for predicted protein structures keyed by UniProt accessions.",
        },
        DatasetRegistryEntry {
            id: "atlas",
            name: "ATLAS",
            domain: "protein_sciences",
            access: "public_metadata",
            status: "implemented",
            source_url: "https://www.dsimb.inserm.fr/ATLAS",
            connector: "atlas_tsv",
            notes: "Reference implementation; local TSV rows are exposed as entry directories with metadata.json.",
        },
        DatasetRegistryEntry {
            id: "mdcath",
            name: "mdCATH",
            domain: "protein_sciences",
            access: "public_bulk",
            status: "sample_connector",
            source_url: "https://github.com/compsciencelab/mdCATH",
            connector: "table",
            notes: "Large MD dataset; expose exported metadata tables first and avoid downloading trajectories by default.",
        },
        DatasetRegistryEntry {
            id: "pdearena",
            name: "PDEArena",
            domain: "pde",
            access: "huggingface",
            status: "planned",
            source_url: "https://huggingface.co/pdearena",
            connector: "huggingface_metadata",
            notes: "Hugging Face organization with PDE datasets; expose dataset cards, splits, and file metadata first.",
        },
        DatasetRegistryEntry {
            id: "pdebench",
            name: "PDEBench",
            domain: "pde",
            access: "github_bulk",
            status: "planned",
            source_url: "https://github.com/pdebench/PDEBench",
            connector: "metadata",
            notes: "Benchmark suite with code and large PDE data; represent tasks and download locations before raw files.",
        },
        DatasetRegistryEntry {
            id: "open_catalyst",
            name: "Open Catalyst",
            domain: "qc_materials",
            access: "bulk_download",
            status: "planned",
            source_url: "https://opencatalystproject.org/",
            connector: "metadata",
            notes: "Large catalyst datasets distributed as task-specific files; expose dataset/task metadata and download links first.",
        },
        DatasetRegistryEntry {
            id: "spice",
            name: "SPICE",
            domain: "qc_materials",
            access: "zenodo",
            status: "planned",
            source_url: "https://zenodo.org/records/10975225",
            connector: "zenodo_metadata",
            notes: "Zenodo-hosted quantum chemistry data; expose record metadata and file manifests first.",
        },
        DatasetRegistryEntry {
            id: "qm9",
            name: "QM9",
            domain: "qc_materials",
            access: "tensorflow_datasets",
            status: "planned",
            source_url: "https://www.tensorflow.org/datasets/catalog/qm9",
            connector: "tfds_metadata",
            notes: "Small-molecule quantum chemistry benchmark available through TensorFlow Datasets.",
        },
        DatasetRegistryEntry {
            id: "physionet",
            name: "PhysioNet",
            domain: "medical",
            access: "mixed_open_restricted",
            status: "planned",
            source_url: "https://physionet.org/about/",
            connector: "api_metadata",
            notes: "Contains open and credentialed datasets; registry must preserve access restrictions.",
        },
        DatasetRegistryEntry {
            id: "chexpert",
            name: "CheXpert",
            domain: "medical",
            access: "restricted_registration",
            status: "planned",
            source_url: "https://stanfordmlgroup.github.io/competitions/chexpert/",
            connector: "metadata",
            notes: "Large chest X-ray dataset requiring registration/terms; expose metadata and access instructions only.",
        },
        DatasetRegistryEntry {
            id: "sleepdata",
            name: "SleepData / NSRR",
            domain: "medical",
            access: "restricted_request",
            status: "planned",
            source_url: "https://sleepdata.org/",
            connector: "metadata",
            notes: "Sleep datasets often require data access requests; expose dataset catalog and request status metadata.",
        },
        DatasetRegistryEntry {
            id: "dandi",
            name: "DANDI",
            domain: "neuroscience",
            access: "public_api_s3",
            status: "planned",
            source_url: "https://docs.dandiarchive.org/api/rest-api/",
            connector: "api_metadata",
            notes: "REST API and public S3-backed assets for Dandisets; good near-term API connector candidate.",
        },
        DatasetRegistryEntry {
            id: "crcns",
            name: "CRCNS",
            domain: "neuroscience",
            access: "mixed_public_account",
            status: "planned",
            source_url: "https://crcns.org/",
            connector: "metadata",
            notes: "Neuroscience sharing portal with browser and command-line downloads; some data may require account/terms.",
        },
        DatasetRegistryEntry {
            id: "microns",
            name: "MICrONS",
            domain: "neuroscience",
            access: "portal_cloud_bulk",
            status: "planned",
            source_url: "https://www.microns-explorer.org/",
            connector: "metadata",
            notes: "Large connectomics and functional imaging resources; expose data product manifests and cloud paths first.",
        },
        DatasetRegistryEntry {
            id: "allen_institute",
            name: "Allen Institute datasets",
            domain: "neuroscience",
            access: "public_api",
            status: "planned",
            source_url: "https://brain-map.org/",
            connector: "api_metadata",
            notes: "Brain-map APIs and SDK access for multiple Allen resources; expose dataset families and API endpoints.",
        },
    ]
}
