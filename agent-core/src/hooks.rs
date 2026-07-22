use brain::Usage;
use std::time::Duration;

/// Estructura que contiene las métricas de ejecución de un turno de conversación.
#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    pub prompt: String,
    pub duration: Duration,
    pub response_len: usize,
    pub usage: Usage,
    pub max_turns: u32,
}

/// Trait para implementar hooks que escuchen la finalización de una ejecución en el agente.
pub trait ExecutionHook: Send + Sync {
    fn on_execution_completed(&self, metrics: &ExecutionMetrics);
}

/// Hook por defecto que imprime en consola el tiempo de ejecución y detalles asociados.
pub struct ConsoleTimeHook;

impl ExecutionHook for ConsoleTimeHook {
    fn on_execution_completed(&self, metrics: &ExecutionMetrics) {
        if metrics.usage.total_tokens > 0 {
            println!(
                "Tiempo: {:.2?} |Tk inp: {}, Tk out: {}, ({} total)",
                metrics.duration,
                metrics.usage.input_tokens,
                metrics.usage.output_tokens,
                metrics.usage.total_tokens
            );
        } else {
            println!("Tiempo: {:.2?}", metrics.duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestHook {
        called: Arc<AtomicBool>,
    }

    impl ExecutionHook for TestHook {
        fn on_execution_completed(&self, _metrics: &ExecutionMetrics) {
            self.called.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_execution_hook_trigger() {
        let called = Arc::new(AtomicBool::new(false));
        let hook = TestHook {
            called: called.clone(),
        };

        let metrics = ExecutionMetrics {
            prompt: "Test".to_string(),
            duration: Duration::from_millis(150),
            response_len: 42,
            usage: Usage::default(),
            max_turns: 10,
        };

        hook.on_execution_completed(&metrics);
        assert!(called.load(Ordering::SeqCst));
    }
}
