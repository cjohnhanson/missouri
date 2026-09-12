# Security policy

## Reporting

Do not open a public issue for a vulnerability.

Report it privately:
**https://github.com/cjohnhanson/missouri/security/advisories/new**

That opens a thread only you and the maintainer can read.

Include what an attacker gains, what they must already control to get
it, the affected commit, and steps that reproduce it.

## What happens next

missouri has one maintainer, so response is best effort. Expect a reply
within a week.

A confirmed report gets a fix and an advisory published together. You
are credited unless you ask otherwise.

## Scope

missouri runs end-to-end tests as directed graphs of filesystem states.
It runs the commands a test declares, and it can drive Docker to contain
them.

A `missouri.yml` file is executable input. A person who runs a suite from
a repository they did not write runs its commands. A report should say
how far one declaration can reach.

In scope:

- A test declaration reaching a path outside the sandbox it was given.
- A comparator or a service command escaping the backend that should
  contain it.
- A request reaching a host or a path that no declaration named.
- Parsing a config file leading to code execution.

Out of scope:

- A dependency advisory with no exploitable path through this tool.
  Report it to that dependency.
- Denial of service from a malformed local file, where the caller
  already controls that file.

## Known boundaries

Missouri runs a transition command on the host, as the user who started
missouri, unless the project config sets `docker: true`. A `packages`
list puts the command in a nix shell, which supplies programs and does
not confine them. Neither is a vulnerability.
