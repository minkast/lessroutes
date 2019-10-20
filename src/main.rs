extern crate clap;
extern crate csv;
extern crate futures;
extern crate ipnet;
extern crate itertools;
extern crate log;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate simplelog;
extern crate tokio;

mod delegation;
mod tree;

use clap::{values_t_or_exit, App, Arg, ArgGroup};
use ipnet::IpNet;
use log::info;
use serde::Serialize;
use simplelog::{Config, LevelFilter, TermLogger, TerminalMode};
use std::{
    collections::HashSet,
    fs::File,
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::default(),
    )?;

    let matches = App::new("lessroutes")
        .version("0.1.0")
        .arg(
            Arg::from_usage("-g, --gateway <gateway>... 'Gateway name and associated countries, e.g. --gateway a=US,JP --gateway b=HK,GB.'")
                .multiple(true)
                .value_delimiter(":"),
        )
        .arg(Arg::from_usage("-4, --output-v4 [output-v4] 'Output file for IPv4 routes.").default_value("routes.v4.json"))
        .arg_from_usage("--no-v4 'Do not generate IPv4 routes.'")
        .arg(Arg::from_usage("-6, --output-v6 [output-v6] 'Output file for IPv6 routes.").default_value("routes.v6.json"))
        .arg_from_usage("--no-v6 'Do not generate IPv6 routes.'")
        .arg(Arg::from_usage("-c, --cache-file [cache-file] 'Cache file for retrieved delegations from registries.'").default_value("delegations.json"))
        .arg_from_usage("--no-cache 'Do not use a cache file.'")
        .arg_from_usage("--update 'Force update delegations from registries.'")
        .arg_from_usage("--no-update 'Do not update delegations from registries.'")
        .group(ArgGroup::with_name("update-control").args(&["update", "no-update"]).conflicts_with("no-cache"))
        .arg_from_usage("--no-default-gateway 'Do not generate route for 0.0.0.0/0 or ::/0.")
        .get_matches();
    let gateways = values_t_or_exit!(matches, "gateway", GatewayMapping);
    let output_v4 = matches.value_of("output-v4").unwrap();
    let no_v4 = matches.is_present("no-v4");
    let output_v6 = matches.value_of("output-v6").unwrap();
    let no_v6 = matches.is_present("no-v6");
    let cache_file = matches.value_of("cache-file").unwrap();
    let no_cache = matches.is_present("no-cache");
    let update = matches.is_present("update");
    let no_update = matches.is_present("no-update");
    let no_default_gateway = matches.is_present("no-default-gateway");

    let delegations;
    if no_cache {
        delegations = delegation::get_delegations().await?;
    } else {
        delegations = delegation::get_delegations_with_cache(cache_file, update, no_update).await?;
    }

    info!("Generating minimum routes.");
    let mut v4_tree = tree::Tree::new(gateways.len());
    let mut v6_tree = tree::Tree::new(gateways.len());
    for (country, ipnets) in &delegations.by_country {
        match gateways
            .iter()
            .enumerate()
            .find(|(_, g)| g.countries.contains(country))
        {
            Some((i, _)) => {
                for ipnet in ipnets {
                    match ipnet {
                        IpNet::V4(ref net) if !no_v4 => v4_tree.mark_v4(net, i + 1),
                        IpNet::V6(ref net) if !no_v6 => v6_tree.mark_v6(net, i + 1),
                        _ => (),
                    }
                }
            }
            None => (),
        }
    }

    if !no_v4 {
        let v4_routes = v4_tree.generate_v4(&gateways, no_default_gateway);
        info!("Writing {} IPv4 routes to {}", v4_routes.len(), output_v4);
        let file = File::create(output_v4)?;
        serde_json::to_writer_pretty(file, &v4_routes)?;
    }
    if !no_v6 {
        let v6_routes = v6_tree.generate_v6(&gateways, no_default_gateway);
        info!("Writing {} IPv6 routes to {}", v6_routes.len(), output_v6);
        let file = File::create(output_v6)?;
        serde_json::to_writer_pretty(file, &v6_routes)?;
    }
    Ok(())
}

#[derive(Debug)]
struct GatewayMapping {
    gateway: String,
    countries: HashSet<String>,
}

impl FromStr for GatewayMapping {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kv: Vec<_> = s.split("=").collect();
        if kv.len() != 2 {
            return Err("missing '='".to_owned());
        }
        let countries = kv[1]
            .split(",")
            .map(|country| {
                let chars: Vec<char> = country.chars().collect();
                if chars.len() != 2
                    || !chars[0].is_ascii_uppercase()
                    || !chars[1].is_ascii_uppercase()
                {
                    return Err(format!("'{}' is not country code", country));
                }
                Ok(country.to_owned())
            })
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(GatewayMapping {
            gateway: kv[0].to_owned(),
            countries,
        })
    }
}

#[derive(Debug, Serialize)]
struct Ipv4Route {
    prefix: Ipv4Addr,
    mask: Ipv4Addr,
    length: u8,
    gateway: String,
}

#[derive(Debug, Serialize)]
pub struct Ipv6Route {
    prefix: Ipv6Addr,
    mask: Ipv6Addr,
    length: u8,
    gateway: String,
}
