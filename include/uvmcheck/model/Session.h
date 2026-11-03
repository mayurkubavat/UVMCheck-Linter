#pragma once

// Loads a design the way the linter does (slang's standard compile options,
// file lists, and the default compat overrides) and exposes it only as a
// uvmcheck::model::Model. This is what the C ABI, and so Rust and Python, sit on.

#include <memory>
#include <string>
#include <vector>

#include "uvmcheck/model/Model.h"

namespace uvmcheck::model {

struct LoadTimings {
    double parseMs = 0;
    double elaborateMs = 0;
    double modelMs = 0;
};

class Session {
public:
    ~Session();
    Session(const Session&) = delete;
    Session& operator=(const Session&) = delete;

    /// `args` are compile options as given to uvmcheck, without the program
    /// name (e.g. {"-f", "files.f", "+incdir+src"}). Never throws; check ok().
    /// Prints nothing; slang's messages go into error(). Not reentrant: do not
    /// run two loads at the same time.
    static std::unique_ptr<Session> load(const std::vector<std::string>& args);

    bool ok() const { return error_.empty(); }
    const std::string& error() const { return error_; }

    const Model& model() const { return *model_; }
    const LoadTimings& timings() const { return timings_; }
    /// Compile errors slang reported (the model is still built).
    size_t errorCount() const { return errorCount_; }

private:
    Session();
    struct Impl;
    std::unique_ptr<Impl> impl_;
    std::unique_ptr<Model> model_;
    LoadTimings timings_;
    size_t errorCount_ = 0;
    std::string error_;
};

} // namespace uvmcheck::model
