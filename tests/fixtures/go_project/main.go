package main

import (
	"fmt"
	"github.com/test/goproject/pkg"
)

type Config struct {
	Port int
}

type Runner interface {
	Run() error
}

func main() {
	config := Config{Port: 8080}
	handler := pkg.NewHandler(config)
	fmt.Println("Starting server")
	serve(handler)
}

func serve(handler Runner) {
	handler.Run()
}
