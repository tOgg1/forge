use std::sync::Mutex;

/// LineRing stores the last N lines of output (ported from Go internal/agent/runner).
#[derive(Debug)]
pub struct LineRing {
    size: usize,
    inner: Mutex<LineRingInner>,
}

#[derive(Debug)]
struct LineRingInner {
    lines: Vec<String>,
    next: usize,
    full: bool,
}

impl LineRing {
    pub fn new(size: usize) -> Self {
        let size = size.max(1);
        Self {
            size,
            inner: Mutex::new(LineRingInner {
                lines: vec![String::new(); size],
                next: 0,
                full: false,
            }),
        }
    }

    pub fn add(&self, line: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if self.size == 0 {
            return;
        }
        let idx = inner.next;
        inner.lines[idx] = line.to_string();
        inner.next += 1;
        if inner.next >= self.size {
            inner.next = 0;
            inner.full = true;
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        let Ok(inner) = self.inner.lock() else {
            return Vec::new();
        };
        if !inner.full {
            return inner.lines[..inner.next].to_vec();
        }
        let mut out = Vec::with_capacity(self.size);
        out.extend_from_slice(&inner.lines[inner.next..]);
        out.extend_from_slice(&inner.lines[..inner.next]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::LineRing;

    #[test]
    fn snapshot_returns_chronological_order() {
        let ring = LineRing::new(3);
        ring.add("a");
        ring.add("b");
        assert_eq!(ring.snapshot(), vec!["a".to_string(), "b".to_string()]);

        ring.add("c");
        assert_eq!(
            ring.snapshot(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );

        ring.add("d");
        assert_eq!(
            ring.snapshot(),
            vec!["b".to_string(), "c".to_string(), "d".to_string()]
        );
    }
}
