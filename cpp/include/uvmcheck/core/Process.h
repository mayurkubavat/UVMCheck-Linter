#pragma once

#include <string>
#include <vector>

namespace uvmcheck::core {

/// Runs argv (looked up on PATH) without a shell, with `extraEnv` entries
/// ("NAME=value") added to or replacing the environment. Collects stdout;
/// stderr is inherited. Returns the exit code, or -1 if it could not run.
int runCapture(const std::vector<std::string>& argv, const std::vector<std::string>& extraEnv,
               std::string& stdoutText);

/// Absolute path of the running uvmcheck binary, or `fallback` (argv[0]) when the
/// platform cannot tell.
std::string selfExecutablePath(const std::string& fallback);

} // namespace uvmcheck::core
