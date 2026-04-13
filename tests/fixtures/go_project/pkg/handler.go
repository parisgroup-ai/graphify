package pkg

import "fmt"

type Handler struct {
	Port int
}

func NewHandler(port int) *Handler {
	return &Handler{Port: port}
}

func (h *Handler) Run() error {
	fmt.Printf("Listening on port %d\n", h.Port)
	return nil
}
