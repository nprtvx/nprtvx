# AirMS threat model

## Trusted

- Device OS and platform secure-storage implementation
- OpenMLS implementation and crypto provider
- Device operator's local state

## Untrusted

- Network relay
- PostgreSQL hosting provider
- Application server operator
- Other application clients except those authenticated by MLS credentials

## Server observations

The relay may observe delivery timing, message size, group identifiers, account metadata, and connection metadata. The server must not be treated as a decryption authority.

## Device identity warning

BasicCredentials are useful for tests, but production account authentication must bind the MLS credential and signature key to a strong identity and key-rotation policy.
