<?php

namespace App\Controllers;

use App\Services\Llm;
use App\Models\User;

class HomeController {
    public function __construct(
        private Llm $llm,
    ) {}

    public function handle(User $user): string {
        log_event("home:handle");
        return $this->llm->call("hello, " . $user->display());
    }
}
