use anyhow::{bail, ensure, Context, Result};

fn get_bit(n: u64, i: usize) -> bool {
    ((1 << i) & n) > 0
}

#[derive(Debug)]
struct NodeChild(Option<Box<Node>>);

impl NodeChild {
    pub fn unbox(&self) -> Option<&Node> {
        self.0.as_ref().map(|boxed| boxed.as_ref())
    }

    pub fn exists(&self) -> bool {
        self.0.is_some()
    }
}

impl From<Node> for NodeChild {
    fn from(inner: Node) -> NodeChild {
        NodeChild(Some(Box::new(inner)))
    }
}

impl std::ops::Deref for NodeChild {
    type Target = Option<Box<Node>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for NodeChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
struct Node {
    value: Option<u64>,
    zero: NodeChild,
    one: NodeChild,
}

impl Node {
    pub fn new(value: Option<u64>) -> Self {
        Self {
            value,
            zero: NodeChild(None),
            one: NodeChild(None),
        }
    }

    pub fn follow(&self, bit: bool) -> Result<&Node> {
        let node = if bit { &self.one } else { &self.zero };

        if node.is_none() {
            if self.is_leaf() {
                bail!(
                    "Attempted to follow from leaf with value {}",
                    self.value.unwrap()
                );
            } else {
                bail!(
                    "Attempted to follow from non-leaf node without child {}",
                    if bit { "one" } else { "zero" }
                );
            }
        }

        let ret = node.as_ref().unwrap().as_ref();
        Ok(ret)
    }

    pub fn add(&mut self, bit: bool, node: Node) -> Result<&Node> {
        let child = if bit { &mut self.one } else { &mut self.zero };

        if child.exists() {
            bail!("child already exists")
        } else {
            *child = node.into();
            child.unbox().context("infallible")
        }
    }

    pub fn follow_or_add(&mut self, bit: bool, value: Option<u64>) -> &mut Node {
        let child = if bit { &mut self.one } else { &mut self.zero };

        child.get_or_insert_with(|| Node::new(value).into())
    }

    pub fn is_leaf(&self) -> bool {
        self.value.is_some()
    }
}

#[derive(Debug)]
pub struct BinaryTrie {
    root: Node,
}

#[derive(Debug)]
pub struct TreeKey(pub u64, pub usize);

impl BinaryTrie {
    pub fn new() -> Self {
        Self {
            root: Node::new(None),
        }
    }

    pub fn add(&mut self, key: TreeKey, value: u64) -> Result<()> {
        let TreeKey(key, len) = key;
        ensure!(len > 0, "key bit length must be positive");
        let mut node = &mut self.root;

        // Key number is matched from MSB to LSB.
        for i in (1..len).rev() {
            if node.is_leaf() {
                // ((1 << i) - 1) should not overflow as the root node must not be
                // leaf node.
                bail!(
                    "cannot add descendant node {} to leaf node {}",
                    key,
                    key & ((1 << i) - 1)
                );
            }
            let bit = get_bit(key, i);
            node = node.follow_or_add(bit, None);
        }

        if node.is_leaf() {
            bail!(
                "cannot add descendant node {} to leaf node {}",
                key,
                key & ((1 << (len - 1)) - 1)
            );
        }

        let bit = get_bit(key, 0);

        // The only circumstance under which this function would fail is when the
        // leaf node to be added already exists. Therefore there are no nodes that
        // were added and should be removed in case of failure.
        node.add(bit, Node::new(Some(value))).context(format!(
            "cannot add leaf node {}, which already exists",
            key
        ))?;

        Ok(())
    }

    pub fn cursor(&self) -> Cursor {
        // It's guaranteed that Tree won't get modified while Cursor lives.
        Cursor { node: &self.root }
    }
}

// TODO: it might be good idea to not let Cursor `follow()` once it fails.
#[derive(Debug)]
pub struct Cursor<'a> {
    node: &'a Node,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NodeType {
    InnerNode,
    LeafNode(u64),
}

impl<'a> Cursor<'a> {
    pub fn follow(&mut self, bit: bool) -> Result<NodeType> {
        let node = self.node.follow(bit)?;
        self.node = node;

        if let Some(v) = self.node.value {
            Ok(NodeType::LeafNode(v))
        } else {
            Ok(NodeType::InnerNode)
        }
    }

    #[allow(unused)]
    pub fn value(self) -> Result<u64> {
        self.node.value.context("not leaf node")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_keys_can_be_added() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b011100, 6), 2).is_ok());
        assert!(trie.add(TreeKey(0b101001, 6), 3).is_ok());
    }

    #[test]
    fn same_key_cannot_be_added() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b010100, 6), 2).is_err());
    }

    #[test]
    fn node_that_already_exists_cannot_be_added() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b010___, 3), 2).is_err());
    }

    #[test]
    fn child_node_of_a_leaf_node_cannot_be_added() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100_, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b0101001, 7), 2).is_err());
    }

    #[test]
    fn descendant_node_of_a_leaf_node_cannot_be_added() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100___, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b010100110, 9), 2).is_err());
    }

    #[test]
    fn cursor_succeeds_for_existent_key() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b110100, 6), 2).is_ok());
        assert!(trie.add(TreeKey(0b110101, 6), 3).is_ok());
        assert!(trie.add(TreeKey(0b10____, 2), 4).is_ok());

        let mut cursor = trie.cursor();

        for i in (1..6).rev() {
            let bit = get_bit(0b010100, i);
            let ret = cursor.follow(bit);
            assert!(ret.is_ok());
            assert_eq!(ret.unwrap(), NodeType::InnerNode);
        }

        let bit = get_bit(0b010100, 0);
        let ret = cursor.follow(bit);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), NodeType::LeafNode(1));

        let value = cursor.value();
        assert!(value.is_ok());
        assert_eq!(value.unwrap(), 1);
    }

    #[test]
    fn cursor_fails_for_non_existent_key() {
        let mut trie = BinaryTrie::new();
        assert!(trie.add(TreeKey(0b010100, 6), 1).is_ok());
        assert!(trie.add(TreeKey(0b110100, 6), 2).is_ok());
        assert!(trie.add(TreeKey(0b110101, 6), 3).is_ok());
        assert!(trie.add(TreeKey(0b10____, 2), 4).is_ok());

        let mut cursor = trie.cursor();

        assert!(cursor.follow(true).is_ok());
        assert!(cursor.follow(false).is_ok());
        assert!(cursor.follow(true).is_err());
    }
}
