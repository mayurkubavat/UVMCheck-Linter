#include "uvmcheck/core/Process.h"

#include <cerrno>
#include <filesystem>
#include <climits>
#include <spawn.h>
#include <string_view>
#include <sys/wait.h>
#include <unistd.h>

#ifdef __APPLE__
#include <mach-o/dyld.h>
#endif

extern char** environ;

namespace uvmcheck::core {

/// Runs argv without a shell; returns the exit code and stdout.
int runCapture(const std::vector<std::string>& argv, const std::vector<std::string>& extraEnv,
               std::string& stdoutText) {
    int pipeFds[2];
    if (pipe(pipeFds) != 0) return -1;
    posix_spawn_file_actions_t actions;
    posix_spawn_file_actions_init(&actions);
    posix_spawn_file_actions_adddup2(&actions, pipeFds[1], STDOUT_FILENO);
    posix_spawn_file_actions_addclose(&actions, pipeFds[0]);

    std::vector<std::string> envStore(extraEnv);
    for (char** e = environ; e && *e; ++e) {
        std::string_view kv(*e);
        bool overridden = false;
        for (const auto& x : extraEnv) {
            if (kv.substr(0, kv.find('=') + 1) == std::string_view(x).substr(0, x.find('=') + 1)) overridden = true;
        }
        if (!overridden) envStore.emplace_back(kv);
    }
    std::vector<char*> envp, args;
    for (auto& s : envStore) envp.push_back(s.data());
    envp.push_back(nullptr);
    std::vector<std::string> argStore(argv);
    for (auto& s : argStore) args.push_back(s.data());
    args.push_back(nullptr);

    pid_t pid = 0;
    int rc = posix_spawnp(&pid, args[0], &actions, nullptr, args.data(), envp.data());
    posix_spawn_file_actions_destroy(&actions);
    close(pipeFds[1]);
    if (rc != 0) {
        close(pipeFds[0]);
        return -1;
    }
    char buf[4096];
    ssize_t n;
    while ((n = read(pipeFds[0], buf, sizeof(buf))) > 0) stdoutText.append(buf, static_cast<size_t>(n));
    close(pipeFds[0]);
    int status = 0;
    while (waitpid(pid, &status, 0) < 0 && errno == EINTR) {}
    return WIFEXITED(status) ? WEXITSTATUS(status) : -1;
}

std::string selfExecutablePath(const std::string& fallback) {
#ifdef __APPLE__
    char buf[PATH_MAX];
    uint32_t size = sizeof(buf);
    if (_NSGetExecutablePath(buf, &size) == 0) return buf;
#else
    std::error_code ec;
    auto exe = std::filesystem::read_symlink("/proc/self/exe", ec);
    if (!ec) return exe.string();
#endif
    return fallback;
}

} // namespace uvmcheck::core
