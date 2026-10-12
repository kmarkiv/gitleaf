# GitLeaf maintenance

## README and website

When a change to this repository’s README is pushed, update the GitLeaf website
in the same task. The owner explicitly requests this manual update alongside
README pushes, rather than a daily sync.

The website repository is `kmarkiv/kmarkiv.github.io`, with GitLeaf at
https://kmarkiv.com/gitleaf/ and its full README guide at
https://kmarkiv.com/gitleaf/guide/. In the local website checkout, run
`python3 scripts/sync_gitleaf.py` after pushing the README, review the generated
content and links, and commit and push those website changes. Update curated
landing-page copy when app behavior or requirements change. Follow the website’s
AGENTS.md and preserve unrelated local changes.

Also refresh the website after publishing a release so download links reflect
newly available assets. Do not advertise an unpublished DMG as available.
