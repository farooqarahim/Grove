# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do not** open a public GitHub issue for security vulnerabilities
2. Use GitHub private vulnerability reporting:
   `https://github.com/farooqarahim/Grove/security/advisories/new`
3. If private vulnerability reporting is unavailable, contact the maintainer privately through GitHub before public disclosure
4. Include a description of the vulnerability and steps to reproduce
5. Allow reasonable time for a fix before public disclosure

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Security Considerations

Grove orchestrates coding agents with access to your local filesystem and git repositories. Keep in mind:

- Grove runs with the permissions of the user who invokes it
- Agent sessions have access to the worktree they are assigned to
- Signing keys and credentials should never be committed to version control
- Use `.gitignore` and environment variables for sensitive configuration
