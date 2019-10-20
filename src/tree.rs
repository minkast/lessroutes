use ipnet::{Ipv4Net, Ipv6Net};
use itertools::Itertools;
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::{GatewayMapping, Ipv4Route, Ipv6Route};

struct Node {
    children: [Option<Box<Node>>; 2],
    color: usize,
    num_routes: Vec<usize>,
    decision: Vec<Option<usize>>,
}

impl Node {
    fn new(colors: usize) -> Node {
        Node {
            children: [None, None],
            color: 0,
            num_routes: vec![std::usize::MAX; colors + 1],
            decision: vec![None; colors + 1],
        }
    }

    fn dp(&mut self) {
        if let [None, None] = self.children {
            assert_ne!(self.color, 0);
            for i in 0..self.num_routes.len() {
                if i == self.color {
                    self.num_routes[i] = 0;
                    self.decision[i] = None;
                } else {
                    self.num_routes[i] = 1;
                    self.decision[i] = Some(i);
                }
            }
        } else {
            assert_eq!(self.color, 0);
            for child in &mut self.children {
                if let Some(child) = child {
                    child.dp();
                }
            }
            for i in 0..self.num_routes.len() {
                let mut routes = 0;
                for child in &self.children {
                    if let Some(child) = child {
                        routes += child.num_routes[i];
                    }
                }
                self.num_routes[i] = routes;
                self.decision[i] = None;
            }
            let mut min_index = 0;
            for i in 1..self.num_routes.len() {
                if self.num_routes[i] < self.num_routes[min_index] {
                    min_index = i;
                }
            }
            for i in 0..self.num_routes.len() {
                if self.num_routes[i] > self.num_routes[min_index] + 1 {
                    self.num_routes[i] = self.num_routes[min_index] + 1;
                    self.decision[i] = Some(min_index);
                }
            }
        }
    }

    fn generate(
        &mut self,
        mut color: usize,
        bits: &mut Vec<bool>,
        colors: &mut Vec<(Vec<bool>, usize)>,
    ) {
        if let Some(new_color) = self.decision[color] {
            colors.push((bits.clone(), new_color));
            color = new_color;
        }
        for (i, child) in self.children.iter_mut().enumerate() {
            if let Some(child) = child {
                bits.push(i != 0);
                child.generate(color, bits, colors);
                bits.pop();
            }
        }
    }
}

pub struct Tree {
    root: Node,
    colors: usize,
}

impl Tree {
    pub fn new(colors: usize) -> Tree {
        Tree {
            root: Node::new(colors),
            colors,
        }
    }

    pub fn mark_v4(&mut self, net: &Ipv4Net, color: usize) {
        self.mark(&net.addr().octets(), net.prefix_len() as usize, color);
    }

    pub fn mark_v6(&mut self, net: &Ipv6Net, color: usize) {
        self.mark(&net.addr().octets(), net.prefix_len() as usize, color);
    }

    fn mark(&mut self, octets: &[u8], prefix_len: usize, color: usize) {
        let bits = octets
            .into_iter()
            .map(|&o| get_bits(o))
            .flatten()
            .take(prefix_len);
        let mut cur = &mut self.root;
        for bit in bits {
            cur = cur.children[bit as usize].get_or_insert(Box::new(Node::new(self.colors)));
        }
        cur.color = color;
    }

    pub(crate) fn generate_v4(&mut self, gateways: &[GatewayMapping], no_default_gateway: bool) -> Vec<Ipv4Route> {
        self.root.dp();
        if no_default_gateway {
            self.root.decision = vec![None; self.colors + 1];
        }
        let mut colors = Vec::new();
        let mut bits = Vec::new();
        self.root.generate(0, &mut bits, &mut colors);

        colors
            .into_iter()
            .map(|(mut bits, color)| {
                let prefix_len = bits.len() as u8;
                bits.resize(32, false);
                let octets: Vec<u8> = bits
                    .into_iter()
                    .chunks(8)
                    .into_iter()
                    .map(|x| get_octet(&x.collect::<Vec<_>>()))
                    .collect();
                let ipnet = Ipv4Net::new(
                    Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]),
                    prefix_len,
                )
                .unwrap();
                Ipv4Route {
                    prefix: ipnet.addr(),
                    mask: ipnet.netmask(),
                    length: prefix_len,
                    gateway: gateways[color - 1].gateway.clone(),
                }
            })
            .collect()
    }

    pub(crate) fn generate_v6(&mut self, gateways: &[GatewayMapping], no_default_gateway: bool) -> Vec<Ipv6Route> {
        self.root.dp();
        if no_default_gateway {
            self.root.decision = vec![None; self.colors + 1];
        }
        let mut colors = Vec::new();
        let mut bits = Vec::new();
        self.root.generate(0, &mut bits, &mut colors);

        colors
            .into_iter()
            .map(|(mut bits, color)| {
                let prefix_len = bits.len() as u8;
                bits.resize(128, false);
                let segments: Vec<u16> = bits
                    .into_iter()
                    .chunks(16)
                    .into_iter()
                    .map(|x| get_segment(&x.collect::<Vec<_>>()))
                    .collect();
                let ipnet = Ipv6Net::new(
                    Ipv6Addr::new(
                        segments[0],
                        segments[1],
                        segments[2],
                        segments[3],
                        segments[4],
                        segments[5],
                        segments[6],
                        segments[7],
                    ),
                    prefix_len,
                )
                .unwrap();
                Ipv6Route {
                    prefix: ipnet.addr(),
                    mask: ipnet.netmask(),
                    length: prefix_len,
                    gateway: gateways[color - 1].gateway.clone(),
                }
            })
            .collect()
    }
}

fn get_bits(x: u8) -> Vec<bool> {
    vec![
        x >> 7 != 0,
        x >> 6 & 1 != 0,
        x >> 5 & 1 != 0,
        x >> 4 & 1 != 0,
        x >> 3 & 1 != 0,
        x >> 2 & 1 != 0,
        x >> 1 & 1 != 0,
        x & 1 != 0,
    ]
}

fn get_octet(bits: &[bool]) -> u8 {
    (bits[0] as u8) << 7
        | (bits[1] as u8) << 6
        | (bits[2] as u8) << 5
        | (bits[3] as u8) << 4
        | (bits[4] as u8) << 3
        | (bits[5] as u8) << 2
        | (bits[6] as u8) << 1
        | bits[7] as u8
}

fn get_segment(bits: &[bool]) -> u16 {
    (get_octet(&bits[..8]) as u16) << 8 | get_octet(&bits[8..]) as u16
}
