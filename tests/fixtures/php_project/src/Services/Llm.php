<?php

namespace App\Services;

class Llm {
    public function call(string $prompt): string {
        log_event("llm:call");
        return "response";
    }

    public function stream(string $prompt): iterable {
        log_event("llm:stream");
        yield "chunk";
    }
}
