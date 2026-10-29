# Local delivery

`Aspect` defines the three supported posting canvases. Posting metadata uses
conservative portable limits (100 UTF-16 title units, 2200 UTF-16 caption units
including hashtags, 5000 UTF-8 bytes, five distinct hashtags). Invalid data is
rejected without truncation. Chapters must begin at zero, increase, and stay
inside the video. Platform-specific chapter eligibility still needs review.

The engine owns source assets, source-guided native reframing, cached intermediate
plates, FilmCraft export jobs, measured loudness, caption copies, per-platform
text files and hashed package inventories. Thumbnail scores describe visible
quality and story fidelity; they do not predict CTR or choose on the user's behalf.

No uploads, OAuth, scheduling or platform clients exist in this crate.
