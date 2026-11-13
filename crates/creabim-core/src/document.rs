use crate::{Element, ElementId, Model, ModelError, Transaction};
use std::sync::atomic::{AtomicU64, Ordering};

/// Identity of one undo step's content. A new id is issued whenever a step is
/// committed or amended, so a persistence layer can tell which steps changed
/// since it last looked by comparing ids instead of whole transactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StepId(u64);

impl StepId {
    /// Ids are unique across every document in the process, so a store
    /// mirroring one document never confuses its steps with another's.
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        StepId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// One entry of the undo or redo stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub id: StepId,
    pub tx: Transaction,
}

impl Step {
    fn new(tx: Transaction) -> Self {
        Self {
            id: StepId::next(),
            tx,
        }
    }
}

/// A model plus its edit history. All edits go through [`Document::commit`].
#[derive(Debug, Default)]
pub struct Document {
    model: Model,
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_model(model: Model) -> Self {
        Self {
            model,
            ..Self::default()
        }
    }

    /// Rebuild a document with its history, e.g. when loading a saved project.
    /// `undo` and `redo` are ordered like the live stacks: the next step to
    /// undo (or redo) is last. Every step is replayed on a scratch copy of the
    /// model first, so a damaged history is reported here rather than making a
    /// later [`Document::undo`] panic.
    pub fn from_parts(
        model: Model,
        undo: Vec<Transaction>,
        redo: Vec<Transaction>,
    ) -> Result<Self, ModelError> {
        let mut scratch = model.clone();
        for tx in undo.iter().rev() {
            tx.inverse().apply(&mut scratch)?;
        }
        for tx in undo.iter().chain(redo.iter().rev()) {
            tx.apply(&mut scratch)?;
        }
        Ok(Self {
            model,
            undo: undo.into_iter().map(Step::new).collect(),
            redo: redo.into_iter().map(Step::new).collect(),
        })
    }

    /// Take the document apart into what [`Document::from_parts`] accepts.
    pub fn into_parts(self) -> (Model, Vec<Transaction>, Vec<Transaction>) {
        let txs = |v: Vec<Step>| v.into_iter().map(|s| s.tx).collect();
        (self.model, txs(self.undo), txs(self.redo))
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The undo stack, oldest first; the next step to undo is last.
    pub fn undo_steps(&self) -> &[Step] {
        &self.undo
    }

    /// The redo stack; the next step to redo is last.
    pub fn redo_steps(&self) -> &[Step] {
        &self.redo
    }

    pub fn commit(&mut self, tx: Transaction) -> Result<(), ModelError> {
        if tx.ops.is_empty() {
            return Ok(());
        }
        let mut tx = tx;
        self.apply_settled(&mut tx)?;
        self.undo.push(Step::new(tx));
        self.redo.clear();
        Ok(())
    }

    /// Apply `tx`, then what keeps walls in place around it (location
    /// lines, heights tied to levels; see [`crate::walls`]), added to `tx`
    /// so it all undoes as one step.
    fn apply_settled(&mut self, tx: &mut Transaction) -> Result<(), ModelError> {
        // Script outputs are locked: a replace would be silently
        // destroyed on the next recompute, so it fails loudly here
        // instead. Only the runner's own transactions (labelled
        // `script…`) may rewrite managed elements; removes are allowed
        // (a deleted output is re-created on the next run, or stays
        // gone with the key). Settle extras appended below are solver
        // consistency, not user intent, and stay exempt.
        if !tx.label.starts_with("script") {
            for op in &tx.ops {
                if let crate::Op::Replace { after, .. } = op {
                    if let Some((_, script)) = crate::script::manager_of(&self.model, after.id) {
                        return Err(ModelError::ManagedByScript {
                            element: after.id,
                            script,
                        });
                    }
                }
            }
        }
        let kept = crate::walls::lines_kept(&self.model, tx);
        tx.apply(&mut self.model)?;
        let levels = crate::walls::touches_walls(tx);
        if !kept.is_empty() || levels {
            let extra = crate::walls::settle(&self.model, &kept, levels);
            if !extra.ops.is_empty() && extra.apply(&mut self.model).is_ok() {
                tx.ops.extend(extra.ops);
            }
        }
        // instances mark what they now set for themselves (see crate::types)
        if crate::types::touches_instances(tx) {
            let extra = crate::types::settle(&self.model, tx);
            if !extra.ops.is_empty() && extra.apply(&mut self.model).is_ok() {
                tx.ops.extend(extra.ops);
            }
        }
        // held dimensions move the other end of what moved (the padlock)
        for round in crate::holds::settle(&self.model, tx) {
            if round.apply(&mut self.model).is_ok() {
                tx.ops.extend(round.ops);
            }
        }
        Ok(())
    }

    /// Convenience: change one element through a closure, as one undo step.
    pub fn edit(
        &mut self,
        id: ElementId,
        label: &str,
        f: impl FnOnce(&mut Element),
    ) -> Result<(), ModelError> {
        let before = self.model.get(id).ok_or(ModelError::NotFound(id))?.clone();
        let mut after = before.clone();
        f(&mut after);
        if after == before {
            return Ok(());
        }
        self.commit(Transaction::new(label).replace(before, after))
    }

    /// Like [`Document::edit`], but merges into the previous undo step when it
    /// edited the same element under the same label. Dragging a value in the
    /// properties panel then undoes in one step, not one per frame.
    pub fn edit_merged(
        &mut self,
        id: ElementId,
        label: &str,
        f: impl FnOnce(&mut Element),
    ) -> Result<(), ModelError> {
        // (walls kept in place by the last edit come after its own op)
        let mergeable = self.redo.is_empty()
            && matches!(self.undo.last(), Some(Step { tx: t, .. }) if t.label == label
                && matches!(t.ops.first(), Some(crate::Op::Replace { after, .. }) if after.id == id)
                && t.ops.iter().all(|o| matches!(o, crate::Op::Replace { .. })));
        if !mergeable {
            return self.edit(id, label, f);
        }
        let current = self.model.get(id).ok_or(ModelError::NotFound(id))?.clone();
        let mut after = current.clone();
        f(&mut after);
        if after == current {
            return Ok(());
        }
        self.commit_merged(Transaction::new(label).replace(current, after))
    }

    /// Commit `tx`, folding it into the last step when that has the same
    /// label and both only replace elements: dragging a value in a field
    /// is then one undo step, even when it changes several elements.
    pub fn commit_merged(&mut self, tx: Transaction) -> Result<(), ModelError> {
        let replaces =
            |t: &Transaction| t.ops.iter().all(|o| matches!(o, crate::Op::Replace { .. }));
        let mergeable = self.redo.is_empty()
            && replaces(&tx)
            && matches!(self.undo.last(), Some(Step { tx: t, .. }) if t.label == tx.label && replaces(t));
        if !mergeable {
            return self.commit(tx);
        }
        if tx.ops.is_empty() {
            return Ok(());
        }
        let mut tx = tx;
        self.apply_settled(&mut tx)?;
        let step = self.undo.last_mut().expect("a step to merge into");
        // what each element was before the first edit, and is now
        let mut befores: Vec<Element> = vec![];
        for o in step.tx.ops.iter().chain(&tx.ops) {
            if let crate::Op::Replace { before, .. } = o {
                if !befores.iter().any(|b| b.id == before.id) {
                    befores.push(before.clone());
                }
            }
        }
        step.tx.ops = befores
            .into_iter()
            .filter_map(|before| {
                let after = self.model.get(before.id)?.clone();
                (after != before).then_some(crate::Op::Replace { before, after })
            })
            .collect();
        step.id = StepId::next();
        if step.tx.ops.is_empty() {
            self.undo.pop();
        }
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.tx.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.tx.label.as_str())
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        step.tx
            .inverse()
            .apply(&mut self.model)
            .expect("undoing a committed transaction cannot fail");
        self.redo.push(step);
        true
    }

    /// Take back the last edit for good, without offering it for redo: for
    /// a drag or a keyboard move that was cancelled.
    pub fn discard_last(&mut self) -> bool {
        let Some(tx) = self.undo.pop() else {
            return false;
        };
        tx.tx
            .inverse()
            .apply(&mut self.model)
            .expect("undoing a committed transaction cannot fail");
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        step.tx
            .apply(&mut self.model)
            .expect("redoing an undone transaction cannot fail");
        self.undo.push(step);
        true
    }
}
