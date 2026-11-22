# Clean advisory candidate

Clean / non-explicit is a shared Smart eligibility filter, not audio editing.
Yes admits only Clean; No admits Explicit and Unknown. Preparation exposes a
manual Clean / Explicit / Unknown override. Overrides use the existing portable
preparation extension `content_advisory` and survive serialization/reanalysis;
embedded tags remain available separately. Titles and filenames are never proof.

Imported Lofty ParentalAdvisory tags (MP4 rtng / Vorbis ITUNESADVISORY) map 2 to
Clean, 1/4 to Explicit, and absent/0/unrecognized/conflicting values to Unknown.
Failed tag refresh clears stale embedded tags, retaining manual classification.
No source audio or tags are written by this feature.

Clean rules require Smart container/sync document version 3 and export version 4.
Legacy rules still load. Old Rust clients reject the unknown field/new version;
the server rejects version downgrade. New desktop sync checks server capability
before sending version 3 and retains the local draft on incompatible servers.
Android continues preserving opaque rule JSON; no Android filter editor is added.
Deploy tested matching server/desktop builds before enabling synchronized Clean
rules. Do not independently enable a new rule on an old-server deployment.

Acceptance still required: exact-commit Windows/Linux full CI; configured Claude
review; staged matching hashes/rollback; controlled in-place runtime verification
of metadata edits, Clean-only selection and roundtrip sync. This document is not
an installation or release receipt. Keep live music running during development.
