#include "uvmcheck/compat/CrashGuard.h"

#include "uvmcheck/compat/Overlay.h"
#include "uvmcheck/core/Diagnostic.h"

#include <cerrno>
#include <csignal>
#include <cstring>
#include <iostream>
#include <string>

#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"

#ifndef _WIN32
#include <fcntl.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

namespace uvmcheck::compat {

using namespace slang::syntax;

namespace {

int stageFd = -1; // child: write end of the stage pipe

std::string_view stageWords(std::string_view stage) {
    if (stage == "parse") return "while parsing";
    if (stage == "elaborate") return "while elaborating the design";
    if (stage == "rules") return "while running rules";
    if (stage == "report") return "while writing the report";
    return "before parsing";
}

/// Finds the syntax patterns of open slang crash reports.
class KnownCrashVisitor : public SyntaxVisitor<KnownCrashVisitor> {
public:
    KnownCrashVisitor(const slang::SourceManager& sm, std::vector<core::Violation>& out) : sm(sm), out(out) {}

    // slang #2066: a class whose type parameter defaults to an enum declared in place.
    void handle(const ClassDeclarationSyntax& cls) {
        if (cls.parameters) {
            for (auto* decl : cls.parameters->declarations) {
                if (decl->kind != SyntaxKind::TypeParameterDeclaration) continue;
                for (auto* d : decl->as<TypeParameterDeclarationSyntax>().declarators) {
                    if (d->assignment && d->assignment->type->kind == SyntaxKind::EnumType) {
                        add(*d, "2066",
                            "a class type parameter whose default is an enum declared in place ('" +
                                std::string(d->name.valueText()) + "')",
                            "declare the enum with a typedef and use its name as the default");
                    }
                }
            }
        }
        visitDefault(cls);
    }

    // slang #2074: an interface with a type parameter that is a virtual interface of itself.
    void handle(const ModuleDeclarationSyntax& decl) {
        if (decl.kind == SyntaxKind::InterfaceDeclaration) {
            std::string_view self = decl.header->name.valueText();
            inInterface.push_back(self);
            visitDefault(decl);
            inInterface.pop_back();
            return;
        }
        visitDefault(decl);
    }

    void handle(const TypeParameterDeclarationSyntax& decl) {
        if (!inInterface.empty()) checkTypeParam(decl, inInterface.back());
    }

private:
    void checkTypeParam(const TypeParameterDeclarationSyntax& decl, std::string_view self) {
        for (auto* d : decl.declarators) {
            if (!d->assignment || d->assignment->type->kind != SyntaxKind::VirtualInterfaceType) continue;
            auto& vif = d->assignment->type->as<VirtualInterfaceTypeSyntax>();
            if (vif.name.valueText() != self) continue;
            add(*d, "2074",
                "a type parameter of interface '" + std::string(self) + "' that is a virtual interface of itself ('" +
                    std::string(d->name.valueText()) + "')",
                "use 'typedef virtual " + std::string(self) + " " + std::string(d->name.valueText()) +
                    ";' instead of a type parameter");
        }
    }

    void add(const SyntaxNode& node, std::string_view issue, const std::string& what, const std::string& fix) {
        auto loc = sm.getFullyOriginalLoc(node.sourceRange().start());
        core::Violation v;
        v.ruleId = "SV-CMP-001";
        v.ruleName = "CompilerError";
        v.severity = core::Severity::Error;
        v.message = "slang is known to crash on " + what + " (slang issue #" + std::string(issue) +
                    ", https://github.com/MikePopoloski/slang/issues/" + std::string(issue) + "); " + fix +
                    ". No other findings could be produced for this run.";
        v.filePath = std::string(sm.getFileName(loc));
        v.line = sm.getLineNumber(loc);
        v.column = sm.getColumnNumber(loc);
        out.push_back(std::move(v));
    }

    const slang::SourceManager& sm;
    std::vector<core::Violation>& out;
    std::vector<std::string_view> inInterface;
};

class CrashGuardOverride : public Override {
public:
    std::string_view id() const override { return "crash-guard"; }
    std::string_view summary() const override {
        return "Parse, elaborate and run rules in a child process; a crash becomes a finding that names the "
               "stage and, for known slang crashes, the upstream issue and a rewrite";
    }
    bool onByDefault() const override { return true; }
};

} // namespace

std::unique_ptr<Override> createCrashGuard() {
    return std::make_unique<CrashGuardOverride>();
}

CrashGuard::Outcome CrashGuard::run() {
    Outcome outcome;
#ifdef _WIN32
    outcome.child = true;
    return outcome;
#else
    int fds[2];
    if (pipe(fds) != 0) {
        outcome.child = true;
        return outcome;
    }
    std::cout.flush();
    std::cerr.flush();
    pid_t pid = fork();
    if (pid < 0) {
        close(fds[0]);
        close(fds[1]);
        outcome.child = true;
        return outcome;
    }
    if (pid == 0) {
        close(fds[0]);
        // Subprocesses the run starts must not hold the pipe open past the run's end.
        fcntl(fds[1], F_SETFD, FD_CLOEXEC);
        stageFd = fds[1];
        outcome.child = true;
        return outcome;
    }
    close(fds[1]);
    // The parent must not take terminal signals meant for the run: the child gets them too.
    std::string stages;
    char buf[256];
    for (;;) {
        ssize_t n = read(fds[0], buf, sizeof(buf));
        if (n > 0) {
            stages.append(buf, static_cast<size_t>(n));
            continue;
        }
        if (n < 0 && errno == EINTR) continue;
        break;
    }
    close(fds[0]);
    int status = 0;
    while (waitpid(pid, &status, 0) < 0) {
        if (errno != EINTR) break;
    }
    if (!stages.empty() && stages.back() == '\n') stages.pop_back();
    size_t nl = stages.rfind('\n');
    outcome.stage = nl == std::string::npos ? stages : stages.substr(nl + 1);
    if (WIFSIGNALED(status)) {
        const int sig = WTERMSIG(status);
        // Interrupts are the user's, not crashes: end the same way.
        if (sig == SIGINT || sig == SIGTERM || sig == SIGHUP || sig == SIGPIPE) {
            signal(sig, SIG_DFL);
            raise(sig);
        }
        outcome.crashed = true;
        outcome.signal = sig;
    } else {
        outcome.exitCode = WIFEXITED(status) ? WEXITSTATUS(status) : 2;
    }
    return outcome;
#endif
}

void CrashGuard::markStage(std::string_view stage) {
#ifndef _WIN32
    if (stageFd < 0) return;
    std::string line(stage);
    line += '\n';
    (void)!write(stageFd, line.data(), line.size());
#else
    (void)stage;
#endif
}

std::vector<core::Violation> CrashGuard::knownCrashPatterns(
    const std::vector<std::shared_ptr<slang::syntax::SyntaxTree>>& trees, const slang::SourceManager& sm) {
    std::vector<core::Violation> out;
    KnownCrashVisitor visitor(sm, out);
    for (const auto& tree : trees) tree->root().visit(visitor);
    return out;
}

core::Violation CrashGuard::crashFinding(const Outcome& outcome, std::vector<core::Violation> known,
                                         const std::string& firstFile) {
    const std::string how = std::string("(signal ") + std::to_string(outcome.signal) + ", " +
                            std::string(stageWords(outcome.stage)) + ")";
    if (!known.empty()) {
        core::Violation v = std::move(known.front());
        v.message = "slang crashed " + how + ". " + v.message;
        return v;
    }
    core::Violation v;
    v.ruleId = "SV-CMP-001";
    v.ruleName = "CompilerError";
    v.severity = core::Severity::Error;
    v.filePath = firstFile;
    v.line = 1;
    v.column = 1;
    if (outcome.stage == "rules" || outcome.stage == "report") {
        v.message = "uvmcheck crashed " + how +
                    "; no findings could be produced. Please report it with the sources that trigger it; "
                    "--engine isolated narrows it to one rule.";
    } else {
        v.message = "slang crashed " + how +
                    "; no findings could be produced. Run with --override -crash-guard to get a core dump, and "
                    "report it at https://github.com/MikePopoloski/slang/issues.";
    }
    return v;
}

} // namespace uvmcheck::compat
