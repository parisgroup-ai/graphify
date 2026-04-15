<?php

namespace App;

use App\Services\Llm;

function bootstrap(): Llm {
    setup_runtime();
    return new Llm();
}
