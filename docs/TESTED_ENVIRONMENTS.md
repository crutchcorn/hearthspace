# Tested environments

Hearthspace has been tested in these environments. This records hands-on use,
not a certification of every device, application, or feature combination.

| System | Processor | RAM | Environment |
| --- | --- | --- | --- |
| MacBook Pro | Apple M4 Max | 64 GB | Ubuntu 26.04 in Parallels |
| ThinkPad T480s | Intel Core i5-8350U | 24 GB | Ubuntu 26.04 |
| GPD Win Max 2 (2025) | AMD Ryzen 370HX | 64 GB | Ubuntu 26.04 |

The MacBook's RAM is the host's capacity, not a stated guest allocation.
Virtualized display/input testing and native hardware testing cover different
paths. A successful run on one system doesn't establish another GPU's behavior,
a complete multi-monitor matrix, or long-term sleep reliability.

When reporting a new result, include the Hearthspace commit, kernel and graphics
driver, display arrangement and scale, input devices, and the workflow tested.
Keep detailed captures and logs with the issue or test artifact. Use this page
as the single inventory of named test systems; feature guides describe how to
repeat the checks.
