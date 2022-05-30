use near_network_primitives::types::Edge;
use near_primitives::network::PeerId;
use smart_default::SmartDefault;
use std::collections::{HashMap, HashSet};
use std::io;
use tracing::debug;

/// TEST-ONLY
#[derive(SmartDefault, Debug, Hash, PartialEq, Eq)]
pub struct Component {
    pub peers: Vec<PeerId>,
    pub edges: Vec<Edge>,
}

/// Everytime a group of peers becomes unreachable at the same time; We store edges belonging to
/// them in components. We remove all of those edges from memory, and save them to database,
/// If any of them become reachable again, we re-add whole component.
///
/// To store components, we have following column in the DB.
/// DBCol::LastComponentNonce -> stores component_nonce: u64, which is the lowest nonce that
///                          hasn't been used yet. If new component gets created it will use
///                          this nonce.
/// DBCol::ComponentEdges     -> Mapping from `component_nonce` to list of edges
/// DBCol::PeerComponent      -> Mapping from `peer_id` to last component nonce if there
///                          exists one it belongs to.
pub struct ComponentStore(schema::Store);
impl ComponentStore {
    pub fn new(store: near_store::Store) -> Self {
        Self(schema::Store(store))
    }

    pub fn push_component(&self, peers: &HashSet<PeerId>, edges: &Vec<Edge>) -> io::Result<()> {
        debug!(target: "network", "try_save_edges: We are going to remove {} peers", peers.len());
        let component = self.0.get::<schema::LastComponentNonce>(&())?.unwrap_or(0) + 1;
        let mut update = self.0.new_update();
        update.set::<schema::LastComponentNonce>(&(), &component);
        update.set::<schema::ComponentEdges>(&component, &edges);
        for peer_id in peers {
            update.set::<schema::PeerComponent>(peer_id, &component);
        }
        update.commit()
    }

    pub fn pop_component(&self, peer_id: &PeerId) -> io::Result<Vec<Edge>> {
        // Fetch the component assigned to the peer.
        let component = match self.0.get::<schema::PeerComponent>(peer_id)? {
            Some(c) => c,
            _ => return Ok(vec![]),
        };
        let edges = self.0.get::<schema::ComponentEdges>(&component)?.unwrap_or(vec![]);
        let mut update = self.0.new_update();
        update.delete::<schema::ComponentEdges>(&component);
        let mut peers_checked = HashSet::new();
        for edge in &edges {
            let key = edge.key().clone();
            for peer_id in [&key.0, &key.1] {
                if !peers_checked.insert(peer_id.clone()) {
                    continue;
                }
                match self.0.get::<schema::PeerComponent>(&peer_id)? {
                    // Store doesn't accept 2 mutations modifying the same row in a single
                    // transaction, even if they are identical. Therefore tracking peers_checked
                    // is critical for correctness, rather than just an optimization minimizing
                    // the number of lookups.
                    Some(c) if c == component => update.delete::<schema::PeerComponent>(&peer_id),
                    _ => {}
                }
            }
        }
        update.commit()?;
        Ok(edges)
    }

    /// TEST-ONLY
    /// Reads all the components from the database.
    /// Panics if any of the invariants has been violated.
    #[allow(dead_code)]
    pub fn snapshot(&self) -> Vec<Component> {
        let edges: HashMap<_, _> =
            self.0.iter::<schema::ComponentEdges>().map(|x| x.unwrap()).collect();
        let peers: HashMap<_, _> =
            self.0.iter::<schema::PeerComponent>().map(|x| x.unwrap()).collect();
        let lcn: HashMap<(), _> =
            self.0.iter::<schema::LastComponentNonce>().map(|x| x.unwrap()).collect();
        // all component nonces should be <= LastComponentNonce
        let lcn = lcn.get(&()).unwrap_or(&0);
        for (c, _) in &edges {
            assert!(c <= lcn);
        }
        for (_, c) in &peers {
            assert!(c <= lcn);
        }
        // Each edge has to be incident to at least one peer in the same component.
        for (c, es) in &edges {
            for e in es {
                let key = e.key();
                assert!(peers.get(&key.0) == Some(c) || peers.get(&key.1) == Some(c));
            }
        }
        let mut cs = HashMap::<u64, Component>::new();
        for (c, es) in edges {
            cs.entry(c).or_default().edges = es;
        }
        for (p, c) in peers {
            cs.entry(c).or_default().peers.push(p);
        }
        cs.into_iter().map(|(_, v)| v).collect()
    }
}

mod schema {
    use borsh::{BorshDeserialize, BorshSerialize};
    use near_network_primitives::types::Edge;
    use near_primitives::network::PeerId;
    use near_store::DBCol;

    pub trait Format<T> {
        fn to_vec(a: &T) -> Vec<u8>;
        fn from_slice(a: &[u8]) -> std::io::Result<T>;
    }

    pub struct U64LE;
    impl Format<u64> for U64LE {
        fn to_vec(a: &u64) -> Vec<u8> {
            a.to_le_bytes().into()
        }
        fn from_slice(a: &[u8]) -> std::io::Result<u64> {
            match a.try_into() {
                Ok(b) => Ok(u64::from_le_bytes(b)),
                Err(_e) => panic!("ff"),
            }
        }
    }

    pub struct Borsh;
    impl<T: BorshSerialize + BorshDeserialize> Format<T> for Borsh {
        fn to_vec(a: &T) -> Vec<u8> {
            a.try_to_vec().unwrap()
        }
        fn from_slice(a: &[u8]) -> std::io::Result<T> {
            T::try_from_slice(a)
        }
    }

    pub trait Column {
        const COL: DBCol;
        type Key;
        type KeyFormat: Format<Self::Key>;
        type Value: BorshSerialize + BorshDeserialize;
    }

    pub struct PeerComponent;
    impl Column for PeerComponent {
        const COL: DBCol = DBCol::PeerComponent;
        type Key = PeerId;
        type KeyFormat = Borsh;
        type Value = u64;
    }

    pub struct ComponentEdges;
    impl Column for ComponentEdges {
        const COL: DBCol = DBCol::ComponentEdges;
        type Key = u64;
        type KeyFormat = U64LE;
        type Value = Vec<Edge>;
    }

    pub struct LastComponentNonce;
    impl Column for LastComponentNonce {
        const COL: DBCol = DBCol::LastComponentNonce;
        type Key = ();
        type KeyFormat = Borsh;
        type Value = u64;
    }

    pub struct Store(pub near_store::Store);
    pub struct StoreUpdate(pub near_store::StoreUpdate);

    impl Store {
        pub fn new_update(&self) -> StoreUpdate {
            StoreUpdate(self.0.store_update())
        }
        pub fn iter<C: Column>(
            &self,
        ) -> impl Iterator<Item = std::io::Result<(C::Key, C::Value)>> + '_ {
            self.0
                .iter(C::COL)
                .map(|(k, v)| Ok((C::KeyFormat::from_slice(&k)?, C::Value::try_from_slice(&v)?)))
        }
        pub fn get<C: Column>(&self, k: &C::Key) -> std::io::Result<Option<C::Value>> {
            self.0.get_ser::<C::Value>(C::COL, C::KeyFormat::to_vec(k).as_ref())
        }
    }
    impl StoreUpdate {
        pub fn set<C: Column>(&mut self, k: &C::Key, v: &C::Value) {
            // set_set fails only if the value cannot be borsh-serialized.
            self.0.set_ser(C::COL, C::KeyFormat::to_vec(k).as_ref(), &v).unwrap()
        }
        pub fn delete<C: Column>(&mut self, k: &C::Key) {
            self.0.delete(C::COL, C::KeyFormat::to_vec(k).as_ref())
        }
        pub fn commit(self) -> std::io::Result<()> {
            self.0.commit()
        }
    }
}
