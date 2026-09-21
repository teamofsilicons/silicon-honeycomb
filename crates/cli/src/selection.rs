use anyhow::{Context, Result, bail};
use honeycomb_client::ReleaseChannel;

/// A release channel is independent of the selected IAM testing environment.
#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    pub app_id: String,
    pub channel: ReleaseChannel,
    pub version: Option<String>,
}

impl Selection {
    pub fn parse(value: &str, version: Option<&str>) -> Result<Self> {
        let (id, suffix) = value
            .split_once('@')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        if let (Some(a), Some(b)) = (suffix, version)
            && a != b
        {
            bail!("The @version and --version selections disagree");
        }
        let (app_id, channel) = id
            .strip_suffix(">test")
            .filter(|base| honeycomb_client::valid_app_id(base))
            .map_or((id, ReleaseChannel::Prod), |id| (id, ReleaseChannel::Dev));
        if !honeycomb_client::valid_app_id(app_id) {
            bail!("Expected 'org>app', 'org>app>test', or either followed by @x.y.z");
        }
        let version = suffix.or(version).map(validate_version).transpose()?;
        Ok(Self {
            app_id: app_id.into(),
            channel,
            version,
        })
    }

    pub fn key(&self) -> String {
        Self::channel_key(&self.app_id, self.channel)
    }

    pub fn channel_key(id: &str, channel: ReleaseChannel) -> String {
        if channel == ReleaseChannel::Dev {
            format!("{id}>test")
        } else {
            id.into()
        }
    }
}

pub fn validate_version(value: &str) -> Result<String> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || !p.bytes().all(|c| c.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
        })
    {
        bail!("Release version must be three numeric components, for example 2.4.1");
    }
    for part in parts {
        part.parse::<u64>()
            .context("Release version component is too large")?;
    }
    Ok(value.into())
}

pub fn channel(value: &str) -> Result<ReleaseChannel, String> {
    match value {
        "prod" => Ok(ReleaseChannel::Prod),
        "dev" => Ok(ReleaseChannel::Dev),
        _ => Err("Use prod or dev".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selector_separates_identity_channel_and_exact_version() {
        let s = Selection::parse("tos>briefcase>test@2.1.0", None).unwrap();
        assert_eq!(s.app_id, "tos>briefcase");
        assert_eq!(s.channel, ReleaseChannel::Dev);
        assert_eq!(s.version.as_deref(), Some("2.1.0"));
        assert_eq!(s.key(), "tos>briefcase>test");
        assert_eq!(
            Selection::parse("tos>briefcase@3.4.2", None)
                .unwrap()
                .channel,
            ReleaseChannel::Prod
        );
        let named_test = Selection::parse("tos>test@1.0.0", None).unwrap();
        assert_eq!(named_test.app_id, "tos>test");
        assert_eq!(named_test.channel, ReleaseChannel::Prod);
        assert_eq!(
            Selection::parse("tos>test>test", None).unwrap().channel,
            ReleaseChannel::Dev
        );
        for value in [
            "tos>app>prod",
            "tos>app>test@1.2",
            "tos>app@1.2.3-dev",
            "tos>app@01.2.3",
            "tos>app@1.2.3@4.5.6",
        ] {
            assert!(Selection::parse(value, None).is_err(), "{value}");
        }
        assert!(Selection::parse("tos>app@1.2.3", Some("2.0.0")).is_err());
    }
}
