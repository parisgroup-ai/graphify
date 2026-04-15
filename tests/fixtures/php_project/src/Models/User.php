<?php

namespace App\Models;

class User {
    public function __construct(
        public readonly string $id,
        public readonly string $name,
    ) {}

    public function display(): string {
        return $this->name;
    }
}
