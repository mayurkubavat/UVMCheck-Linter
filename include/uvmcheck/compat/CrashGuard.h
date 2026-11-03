#pragma once

#include <memory>
#include <string>
#include <string_view>
#include <vector>

namespace slang {
class SourceManager;
}
namespace slang::syntax {
class SyntaxTree;
}
namespace uvmcheck::core {
struct Violation;
}

namespace uvmcheck::compat {

/// Runs the parse, elaborate and rule stages in a child process, so a crash in
/// slang (or in a rule) becomes a finding instead of a lost run. The parent
/// waits; when the child ends normally the parent exits with its status and
/// prints nothing. When the child dies on a signal, the parent goes on to
/// report the crash. Used by the `crash-guard` override.
class CrashGuard {
public:
    struct Outcome {
        bool child = false;   ///< This is the child: carry on and do the work.
        bool crashed = false; ///< Parent only: the child died on a signal.
        int exitCode = 0;     ///< Parent only: the child's exit status when it did not crash.
        int signal = 0;       ///< Parent only: the signal that ended the child.
        std::string stage;    ///< Parent only: the last stage the child entered.
    };

    /// Forks. Returns {child = true} in the child. In the parent, waits for the
    /// child and returns how it ended. When fork is unavailable the caller runs
    /// unguarded: returns {child = true}.
    static Outcome run();

    /// In the child, records the stage it enters ("parse", "elaborate", "rules",
    /// "report") so the parent can say where a crash happened. No-op elsewhere.
    static void markStage(std::string_view stage);

    /// Known slang crashes recognisable from syntax alone, each naming the
    /// upstream issue and a rewrite that avoids it.
    static std::vector<core::Violation> knownCrashPatterns(
        const std::vector<std::shared_ptr<slang::syntax::SyntaxTree>>& trees, const slang::SourceManager& sm);

    /// The finding reported for a crash: a known pattern when one is found, else
    /// a generic one at the first source file.
    static core::Violation crashFinding(const Outcome& outcome, std::vector<core::Violation> known,
                                        const std::string& firstFile);
};

} // namespace uvmcheck::compat
