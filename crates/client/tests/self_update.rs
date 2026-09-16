#[cfg(unix)]
mod unix {
    use sha2::{Digest, Sha256};
    use silicon_honeycomb_client::maintenance::install_cli_release;
    fn archive(script: &[u8], link: bool) -> Vec<u8> {
        let gzip = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        if link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("/tmp/foreign").unwrap();
            header.set_size(0);
            header.set_cksum();
            tar.append_data(&mut header, "honeycomb", std::io::empty())
                .unwrap();
        } else {
            header.set_size(script.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, "honeycomb", script).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }
    #[tokio::test]
    async fn replacement_requires_checksum_regular_file_and_matching_version() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("honeycomb");
        std::fs::write(&binary, "original").unwrap();
        let bytes = archive(b"#!/bin/sh\nprintf 'honeycomb 9.0.0\\n'\n", false);
        let checksum = hex::encode(Sha256::digest(&bytes));
        assert!(
            install_cli_release(&bytes, &"0".repeat(64), "honeycomb 9.0.0", &binary)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&binary).unwrap(), "original");
        assert!(
            install_cli_release(&bytes, &checksum, "honeycomb 10.0.0", &binary)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&binary).unwrap(), "original");
        let linked = archive(b"", true);
        assert!(
            install_cli_release(
                &linked,
                &hex::encode(Sha256::digest(&linked)),
                "honeycomb 9.0.0",
                &binary
            )
            .await
            .is_err()
        );
        install_cli_release(&bytes, &checksum, "honeycomb 9.0.0", &binary)
            .await
            .unwrap();
        assert_eq!(
            std::process::Command::new(binary)
                .arg("--version")
                .output()
                .unwrap()
                .stdout,
            b"honeycomb 9.0.0\n"
        );
    }
    #[tokio::test]
    async fn managed_command_link_and_pinned_binary_survive_update() {
        let directory = tempfile::tempdir().unwrap();
        let pinned = directory.path().join("pinned-honeycomb");
        let link = directory.path().join("honeycomb");
        std::fs::write(&pinned, "pinned release").unwrap();
        std::os::unix::fs::symlink(&pinned, &link).unwrap();
        let bytes = archive(b"#!/bin/sh\nprintf 'honeycomb 9.0.0\\n'\n", false);
        let checksum = hex::encode(Sha256::digest(&bytes));
        let error = install_cli_release(&bytes, &checksum, "honeycomb 9.0.0", &link)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("managed command link"));
        assert_eq!(std::fs::read_link(&link).unwrap(), pinned);
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "pinned release");
    }
}
