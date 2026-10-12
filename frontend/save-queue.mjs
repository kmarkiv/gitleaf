// Serialize saves and repeat when newer revisions arrive during an in-flight request.
export class SaveQueue {
  constructor({ snapshot, write, commit, failed }) {
    Object.assign(this, { snapshot, write, commit, failed });
    this.pending = null;
  }
  save() {
    if (this.pending) return this.pending;
    this.pending = this.flush().finally(() => { this.pending = null; });
    return this.pending;
  }
  async flush() {
    for (let round = 0; round < 10; round++) {
      const documents = this.snapshot();
      if (!documents.length) return true;
      for (const document of documents) {
        try { await this.write(document); this.commit(document); }
        catch (error) { this.failed(document, error); return false; }
      }
    }
    // A caller such as Compile must not proceed using an unsaved revision.
    return this.snapshot().length === 0;
  }
}
