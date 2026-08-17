---
type: governance
title: Website Delivery
description: Defines the product website, search metadata, verified native installers, and GitHub Pages deployment boundary.
tags: [governance, website, seo, installer, github-pages]
status: draft
stale_after: 2027-02-16
generated: { by: bahadirarda, at: 2026-08-16T16:10:56Z }
sources:
  - id: github-pages-workflows
    resource: https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages
    title: Using custom workflows with GitHub Pages
  - id: google-search-developers
    resource: https://developers.google.com/search/docs/fundamentals/get-started-developers
    title: SEO Guide for Web Developers
  - id: google-sitemaps
    resource: https://developers.google.com/search/docs/crawling-indexing/sitemaps/build-sitemap
    title: Build and submit a sitemap
  - id: llms-text-proposal
    resource: https://llmstxt.org/
    title: The llms.txt proposal
---

# Website Delivery

## Product surface

The canonical product website is `https://bahadirarda.github.io/pkgshift/`. Its source lives under `site/` as dependency-free semantic HTML, CSS, and progressive JavaScript. The website remains readable, navigable, and installable without client-side rendering.

The visual system reuses the repository's black, paper, signal-orange, ochre, and olive identity. The tracked brand banner becomes the social preview image during deployment rather than being duplicated in source control.

## Search contract

The page exposes one descriptive title, one concise description, a canonical URL, crawl directives, Open Graph metadata, large-image social metadata, and `SoftwareApplication` structured data. The static `robots.txt` references the root `sitemap.xml`, and the sitemap contains only canonical indexable URLs.[^google-search-developers][^google-sitemaps]

Important product language remains visible in semantic HTML. Search metadata does not claim package manager support, safety properties, or installation behavior beyond the product contract documented elsewhere in this bundle.

The root `llms.txt` follows the community proposal for a concise Markdown project summary followed by curated file lists.[^llms-text-proposal] It points agents to raw repository documentation, the portable Agent Skill, the verified installer, releases, and source. This is an agent discovery aid rather than a search ranking claim or access-control mechanism.

## Installer contract

`site/install.sh` for Linux and macOS and `site/install.ps1` for Windows x86-64 implement the same native installer contract. They:

1. Detects the operating system and architecture.
2. Resolves either the latest stable release or an explicitly pinned stable version.
3. Downloads the matching native archive and `SHA256SUMS` from the same GitHub Release.
4. Verifies the archive checksum before extraction.
5. Validate the archive's `release.json` name, version, tag, and target before staging installation.
6. Stage and replace the canonical portable Agent Skill under the selected shared data root without retaining stale release files.
7. Stage and replace the executable under an explicit or platform user-owned destination.
8. Execute the installed binary's version check and confirm that the Rust Skill lifecycle resolves the installed portable source before reporting success.
9. Restore the previous binary and Skill data when an activated replacement fails either smoke check.

Neither script requires elevated privileges, edits a shell profile, runs the migrated project, or installs dependencies. Unix defaults are `XDG_BIN_HOME` or `$HOME/.local/bin` for the executable and `PKGSHIFT_DATA_DIR`, `XDG_DATA_HOME/pkgshift`, or `$HOME/.local/share/pkgshift` for shared data. Windows defaults are `%LOCALAPPDATA%\pkgshift\bin` and `%LOCALAPPDATA%\pkgshift`; explicit `PKGSHIFT_INSTALL_DIR` and `PKGSHIFT_DATA_DIR` values override them. Continuous integration runs real fixture archives through both installers, verifies stale Skill cleanup, and proves that checksum or smoke-check failure preserves the installed version.

## Deployment

The `pages` workflow validates repository metadata, assembles an isolated static artifact, copies the tracked social card, uploads the Pages artifact, and deploys through the `github-pages` environment. The deployment job receives only `pages: write` and `id-token: write` in addition to read-only repository content, matching the GitHub Pages custom workflow contract.[^github-pages-workflows]

Website source changes run through ordinary pull request validation before the default-branch deployment. The Pages workflow repeats structural validation so an invalid canonical URL, missing asset, malformed manifest, or unsafe installer contract blocks publication.

[^github-pages-workflows]: GitHub documents `configure-pages`, `upload-pages-artifact`, the `github-pages` environment, and `deploy-pages` as the custom workflow boundary.
[^google-search-developers]: Google recommends descriptive titles and descriptions, semantic HTML, crawlable links, accessible content, and structured data for search-facing sites.
[^google-sitemaps]: Google recommends absolute canonical URLs in a root sitemap.
[^llms-text-proposal]: The llms.txt proposal defines an H1, an optional blockquote summary, explanatory content, and H2-delimited link lists.
