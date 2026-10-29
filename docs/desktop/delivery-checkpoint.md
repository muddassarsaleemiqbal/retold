# Native export review checkpoint

The video overview now has an explicit Export action. Its separate surface selects one
saved edition, landscape or portrait format, and editor-project preparation or video
rendering. Project-only is the initial choice. These choices are not restored on launch.

The typed application review has private authority fields and a separate export factory.
It pins the export scope and exact backend plan with its separate spend/request limits; normal
run review continues to reject export options. The review lists prepared deliveries,
the destination directory, stages and blockers. Missing ready output cannot be generated
by an export. The shared backend still owns content approval, account and scope checks.

Changing an edition, format or output mode removes confirmation. Script operations and
provider saves also invalidate an export review. Unsaved script text blocks entry into
export until saved or discarded. A confirmed request uses a stable export idempotency key,
recompiles the plan and checks the reviewed hash before queue admission. It never starts a
worker implicitly. Submission acknowledgement leads to Activity and is not presented as
finished output.

Export limits start at zero and never inherit a run allowance. Final vision QA remains
enabled: users can explicitly authorize capped provider work on this export. Changing
either limit removes confirmation. Edition choices use the engine's delivery identifiers,
including burned-caption distinctions, rather than full caption specifications.
Confirmation also compares the current fields against the immutable reviewed request
before submitting, independently of input-event presentation updates.

Native fixtures cover blocked project and render confirmation at 1024×700 and 760×640,
light/dark rendering, scope-change invalidation and no queue creation. Typed checks show
that project-only and render plans have different hashes, default to zero paid allowance and
cannot submit an unprepared video. The existing shared/CLI production fixtures exercise
real editor-project and rendered delivery; they are separate evidence from native tests.

The standalone desktop gate also reuses the existing zero-price loopback production fixture:
the CLI prepares ready output and explicitly starts its worker, then native controls choose,
review and confirm an export. The test requires exactly one completed job and FFprobe-verified
video and audio streams in the resulting MP4. This passed on the development Mac. Captures
are retained in `target/gpui-g0/evidence/native-production`; no live provider is used.

The [saved-draft native generation journey](character-review.md) now reaches ready through
native character approval and then exports. Native source-backed draft creation and
packaged-worker export remain unqualified.
Verified-output browsing, preset/custom-format coverage, multi-edition selection and
platform packaging remain G9 work. This checkpoint does not claim G9 or full GUI completion.

Final checkpoint validation (2026-10-26): the full shared `scripts/check.sh` and standalone
desktop `experiments/retold-desktop/check.sh` both exited 0. This includes the native positive
export journey, native interaction/rendering, packaged-worker checks, unchanged Clean Code
baseline, and pinned desktop dependency policy. Logs: `.git/retold-delivery-shared-resume.log`
and `.git/retold-delivery-desktop-resume.log`. The native review capture was visually inspected.
