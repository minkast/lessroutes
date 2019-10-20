use csv::ReaderBuilder;
use futures::future::try_join_all;
use ipnet::{IpAdd, IpNet, Ipv4Subnets, Ipv6Net};
use itertools::Itertools;
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::{
    process::exit,
    collections::HashMap,
    fs::{self, File},
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};

const APNIC_DELEGATION: &'static str = "https://ftp.apnic.net/stats/apnic/delegated-apnic-latest";
const ARIN_DELEGATION: &'static str =
    "https://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest";
const RIPE_DELEGATION: &'static str =
    "https://ftp.ripe.net/pub/stats/ripencc/delegated-ripencc-extended-latest";
const LACNIC_DELEGATION: &'static str =
    "https://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-latest";
const AFRINIC_DELEGATION: &'static str =
    "https://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-latest";

#[derive(Debug, Serialize, Deserialize)]
pub struct Delegations {
    pub by_country: HashMap<String, Vec<IpNet>>,
}

pub async fn get_delegations_with_cache(
    cache_file: &str,
    update: bool,
    no_update: bool,
) -> Result<Delegations, Box<dyn std::error::Error>> {
    let need_update = update || match fs::metadata(cache_file) {
        Ok(meta) => !no_update && match meta.modified() {
            Ok(mtime) => match mtime.elapsed() {
                Ok(duration) => duration > Duration::from_secs(24 * 3600),
                Err(_) => false, // mtime is later than now.
            },
            Err(_) => false, // mtime is not support on this platform.
        },
        Err(_) => {
            // cache file does not exist.
            if no_update {
                error!("Cache file is not present but --no-update is specified.");
                exit(1);
            }
            true
        }
    };

    if need_update {
        let delegations = get_delegations().await?;
        info!("Caching delegations on disk.");
        let file = File::create(cache_file)?;
        serde_json::to_writer(file, &delegations)?;
        Ok(delegations)
    } else {
        info!("Loading delegations from cache.");
        let file = File::open(cache_file)?;
        let delegations: Delegations = serde_json::from_reader(file)?;
        Ok(delegations)
    }
}

pub async fn get_delegations() -> Result<Delegations, Box<dyn std::error::Error>> {
    info!("Downloading latest delegations from registries.");
    let client = reqwest::Client::builder().use_sys_proxy().build()?;
    let mut pairs:Vec<(String, IpNet)> = try_join_all(
        vec![
            ("apnic", APNIC_DELEGATION),
            ("arin", ARIN_DELEGATION),
            ("ripe", RIPE_DELEGATION),
            ("lacnic", LACNIC_DELEGATION),
            ("afrinic", AFRINIC_DELEGATION),
        ]
        .into_iter()
        .map(|(registry, url)| download_delegations(registry, &client, url)),
    )
    .await?
    .into_iter()
    .flatten()
    .collect();
    pairs.sort_by_key(|(country, _)|country.to_owned());

    let by_country = pairs.into_iter().group_by(|(country, _)| country.to_owned())
    .into_iter()
    .map(|(country, ipnets)| (country, ipnets.map(|(_, nets)| nets).collect::<Vec<_>>()))
    .collect::<HashMap<_, _>>();
    Ok(Delegations { by_country })
}

async fn download_delegations(
    registry: &str,
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<(String, IpNet)>, Box<dyn std::error::Error>> {
    info!("Downloading delegations from {}.", registry);
    let resp = client.get(url).send().await?;
    let resp = resp.error_for_status()?;
    let content = resp.text().await?;
    info!("Downloaded delegations from {}.", registry);

    let mut rdr = ReaderBuilder::new()
        .comment(Some(b'#'))
        .delimiter(b'|')
        .flexible(true)
        .from_reader(content.as_bytes());
    let records = rdr.records().collect::<Result<Vec<_>, _>>()?;
    Ok(records
        .into_iter()
        .filter(|r| {
            r.get(5) != Some("summary")
                && (r.get(2) == Some("ipv4") || r.get(2) == Some("ipv6"))
                && (r.get(6) == Some("allocated") || r.get(6) == Some("assigned"))
        })
        .map(|r| {
            let country = r.get(1).unwrap();
            let length = r.get(4).unwrap().parse::<usize>().unwrap();
            let start = r.get(3).unwrap();
            let ipnets = match r.get(2).unwrap() {
                "ipv4" => {
                    let start: Ipv4Addr = start.parse().unwrap();
                    let end = start.saturating_add((length - 1) as u32);
                    Ipv4Subnets::new(start, end, 0).map(IpNet::V4).collect()
                }
                "ipv6" => vec![IpNet::V6(
                    Ipv6Net::new(start.parse::<Ipv6Addr>().unwrap(), length as u8).unwrap(),
                )],
                _ => panic!("unknown type"),
            };
            ipnets
                .into_iter()
                .map(|ipnet| (country.to_owned(), ipnet))
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect())
}
