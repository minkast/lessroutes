lessroutes
====

![crates.io](https://img.shields.io/crates/v/lessroutes.svg)

Generate a minimal set of routes based on country. Both IPv4 and IPv6 are supported. Inspired by [bestroutetb](https://github.com/ashi009/bestroutetb).

## Building

First, install Rust toolchain with https://rustup.rs/. Then, you can build and install lessroutes with cargo.

```shell
$ cargo install --path .
```

## Usage

Assume you want to route traffic to the US and Japan to gateway A, and traffic to Hong Kong and the UK to gateway B. You will specify the `--gateway` argument as `lessroutes --gateway a=US,JP --gateway b=HK,GB`. The routes will be stored in `routes.v4.json` and `routes.v6.json`, which contain IPv4 and IPv6 routes respectively. The format of these files is like:

```json
[
    ...
    {
        "prefix": "1.2.0.0",
        "mask": "255.255.0.0",
        "length": 16,
        "gateway": "a"
    },
    ...
]
```

Where `<prefix>/<length>` forms an CIDR of a net block, and `mask` is the network mask of that block.

Specify `--output-v4 <file>` or `--output-v6 <file>` to change the default output file name.

Specify `--no-v4` if you don't want IPv4 routes, or `--no-v6` if you don't want IPv6 routes.

Specify `--cache-file <file>` to change the default cache file name, or `--no-cache` to not use a cache file.

Specify `--update` to force update the cache or `--no-update` to force use the cache.

By default, lessroutes generates routes for `0.0.0.0/0` or `::/0` if needed, you can specify `--no-default-gateway` to not generate them.

## License

[MIT](https://opensource.org/licenses/MIT)
