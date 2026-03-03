# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| > 1.6.0 | :white_check_mark: |
| < 1.6.0 | :x:                |

Only the versions listed above receive security updates. If you are using an unsupported version, upgrade before reporting.

---

## Reporting a Vulnerability

If you discover a security vulnerability, **do not disclose it publicly**.

**Do NOT:**

* Open a GitHub issue describing the vulnerability.
* Create a pull request that references or demonstrates the vulnerability.
* Post proof-of-concept code, stack traces, logs, or screenshots in public discussions.
* Discuss the vulnerability in public forums, chat rooms, or social media.

Public disclosure before a coordinated fix may put users at risk.

Instead, report the issue privately using one of the following methods:

1. **GitHub Security Advisory (preferred):**
   Go to the repository’s **Security** tab and click **“Report a vulnerability”** to submit a private report.

2. **Email:**
   Send details to **[stratumv2@gmail.com](mailto:stratumv2@gmail.com)**.


When reporting, include:

* A clear description of the vulnerability.
* Steps to reproduce.
* A minimal proof-of-concept (if applicable).
* Affected version(s).
* Any suggested mitigations or fixes (optional).

---

## Secure Communication

The following OpenPGP keys may be used to encrypt sensitive information:

| Name              | Fingerprint                                       |
| ----------------- | ------------------------------------------------- |
| Gabriele Vernetti | 0345 C089 6857 29DC 5D31 CED1 6FCC 67B5 D080 8BAD |
| plebhash          | 94CF 981D D645 F635 49F2 E169 F10E C756 6A81 2D52 |

To import a key:

```
gpg --keyserver hkps://keys.openpgp.org --recv-keys "<fingerprint>"
```

Verify the fingerprint before encrypting sensitive data.

---

## Disclosure Policy

* We will acknowledge receipt of your report as soon as possible.
* We will investigate and determine impact and remediation steps.
* We may request additional information during triage.
* Once a fix is prepared, we will coordinate responsible disclosure.
* Credit will be given if desired.

### If You Also Have a Fix

Do **not** open a pull request referencing the vulnerability.

Send the proposed patch privately via email **or** attach it directly to the private GitHub Security Advisory thread (via the Security → “Report a vulnerability” workflow).
We will coordinate review, integration, and disclosure timing before any public reference is made.

