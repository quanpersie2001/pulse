# Where the prose goes

The records are the truth layer and the trace; docs are where a human
learns the product. Four files take a shaped Story's prose.

- `docs/product/<capability>.md` (frontmatter `applies_to`/`tags`) reads as
  the product's current behavior, told in one pass: what the capability is
  for, then its rules and exceptions by id, then the flows a user or caller
  goes through. Every sentence is in the present tense and true today. A
  rule this Story changed is rewritten where it already sits, carrying its
  id; a rule this Story adds joins the section it belongs to by behavior.
  The test: a reader who has never seen the backlog reads it top to bottom
  and knows what the product does.
- `docs/decisions/<DEC-id>-<slug>.md`: one file per Decision record —
  question, options considered, the decision, consequences, and the
  capability it shaped. This is where "why" lives; the product doc states
  only "what".
- `docs/domain/glossary.md`: one line per term the interview settled, when
  new vocabulary appeared.
- `docs/README.md`: one line per new doc.


## Folding a Story into a capability doc

The doc is organised by product behavior, not by the order Stories landed.
So a Story's rules do not arrive as a new section — they join the sections
that already describe that behavior:

- a rule this Story **changed** is rewritten where it already sits, keeping
  its id, so the file never carries two versions of one rule;
- a rule this Story **adds** joins the section its behavior belongs to;
- a capability this Story **introduces** gets its own file.

The test is a reader, not a checklist: someone who has never seen the
backlog reads the file top to bottom and knows what the product does. If a
sentence only makes sense to someone who remembers the Story, rewrite it.
