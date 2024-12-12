use futures::future;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use tokio::{self, task::JoinHandle};

use uuid;

#[derive(Debug)] // Cannot derive Clone because oneshot::Sender doesn't implement Clone
enum Message {
    TextMessage {
        id: uuid::Uuid,
        body: String,
    },
    IncrementCounter {
        by: u32,
    },
    Stop {
        tx: tokio::sync::oneshot::Sender<()>,
    },
}

#[derive(Debug)]
enum ActorState {
    Ready,
    Running,
    Stopped,
}

#[derive(Debug)]
struct Actor {
    // Responsible for processing data
    // Should run in its own thread in the background
    id: uuid::Uuid,
    rx: tokio::sync::mpsc::Receiver<Message>, // Can't derive `Clone` for `mpsc::Receiver`


    state: Arc<Mutex<RefCell<ActorState>>>,   // Requires `Mutex` to be shared between threads
    // Cannot use RwLock since it's not Sync and cannot be safely shared among threads.
    // state: Arc<tokio::sync::RwLock<RefCell<ActorState>>>

    // Actor's data
    counter: u32,
}

#[derive(Debug, Clone)]
struct ActorHandler {
    // Hard-linked to an actor, which doe the actual computation
    id: uuid::Uuid,
    tx: tokio::sync::mpsc::Sender<Message>,
    state: Arc<Mutex<RefCell<ActorState>>>, // Needs Arc<Mutex<RefCell<...>>> because state is shared with `Actor`
                                            // Can't derive `Clone` for `JoinHandle`
                                            // My idea was to store the handle here so that
                                            // we could wait for the actor to finish processing messages
                                            // -> handle: Option<JoinHandle<()>>,
}

impl Actor {
    async fn run(&mut self) {
        println!("[Actor.run] Actor {} is running", self.id);
        // Can use `RefCell`'s `replace` to change the state of the actor
        self.state.lock().unwrap().replace(ActorState::Running);

        // Consume messages while the channel is open
        while let Some(message) = self.rx.recv().await {
            self.process(message).await;
        }

        self.state.lock().unwrap().replace(ActorState::Stopped);

        println!("[Actor.run] Actor {} is shutting down...", self.id);
    }

    async fn process(&mut self, message: Message) {
        match message {
            Message::TextMessage { id, body } => {
                println!(
                    "[Actor.process] Actor {} received message:\n\t{:?}",
                    self.id, Message::TextMessage { id, body }
                );
                self.counter += 1;
                println!("\tcounter: {}.", self.counter);
            }
            Message::IncrementCounter { by } => {
                self.counter += by;
                println!("[Actor.process] Actor {} received message: {:?}", self.id, Message::IncrementCounter { by });
                println!("\tcounter: {}", self.counter);
            }
            Message::Stop { tx } => {
                println!("[Actor.process] Actor {} received stop message", self.id);
                self.state.lock().unwrap().replace(ActorState::Stopped);

                // Acknowledge handler that actor has stopped.
                // Since this is a oneshot channel it gets dropped right away
                let _ = tx.send(());

                // Close actor's channel
                self.rx.close();
            }
        }
    }
}

impl ActorHandler {

    // Creates an Actor and starts it in a separate thread.
    // Returns an ActorHandler instance and a thread handle,
    // which belong to the Actor which is consuming messages.
    // This allows waiting for the actor to finish processing all the messages.
    fn new(buffer: usize) -> (Self, JoinHandle<()>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        let actor_id = uuid::Uuid::new_v4();
        let state = Arc::new(Mutex::new(RefCell::new(ActorState::Ready)));
        let actor_state = state.clone();

        println!("[ActorHandler::new] Creating new actor: {}", actor_id);

        let actor_handler = ActorHandler {
            id: actor_id,
            tx: tx,
            state: state,
        };

        // Can use std::thread::spawn instead if we want to run the actor in a dedicated thread.
        // Can come handy when `process` does blocking IO.
        let handle = tokio::spawn(async move {
            let mut actor = Actor {
                id: actor_id,
                rx: rx,
                state: actor_state,
                counter: 0,
            };
            // yield the control to the tokio runtime (for illustrative purposes, makes output more _unordered_)
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            actor.run().await;
        });

        (actor_handler, handle)
    }

    fn send(
        &self,
        message: Message,
    ) -> JoinHandle<Result<(), tokio::sync::mpsc::error::SendError<Message>>> {
        // Validate that the actor is active.
        match *self.state.lock().unwrap().borrow_mut() {
            ActorState::Stopped => {
                println!("[ActorHandler::send] Actor {} is stopped", self.id);
                return tokio::spawn(async { Err(tokio::sync::mpsc::error::SendError(message)) });
            }
            _ => {}
        }

        let tx = self.tx.clone();

        // Send message on separate thread.
        let handle = tokio::spawn(async move {
            tx.send(message).await
        });

        // Return handle so that thread can be joined.
        handle
    }

    // Stop the actor.
    // This is similar to the `send` method, but does not require passing an instance
    // of `Message::stop` nor for that method to process the ack or for the ack to be processed somewhere else.
    async fn stop(&self) -> JoinHandle<Result<(), tokio::sync::mpsc::error::SendError<Message>>> {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let message = Message::Stop { tx: tx };

        match *self.state.lock().unwrap().borrow_mut(){
            ActorState::Stopped => {
                println!("[ActorHandler.stop] Actor {} is stopped", self.id);
                return tokio::spawn(async { Err(tokio::sync::mpsc::error::SendError(message)) });
            }
            _ => {}
        }

        println!("[ActorHandler.stop] Stopping actor: {}", self.id);

        let handle = self.send(message);

        // Block actor. Can spawn a thread for non blocking behavior.
        let _ = rx.await;

        println!("[ActorHandler.stop] Acknowledged! Actor {} has stopped", self.id);

        handle
    }
}

#[tokio::main]
async fn main() {
    let (actor1, actor1_worker) = ActorHandler::new(10);
    let (actor2, actor2_worker) = ActorHandler::new(10);

    println!("[main] Actors created");
    println!("[main] Actor1: {:?}", actor1);
    println!("[main] Actor2: {:?}", actor2);

    // Create messages
    let messages_actor1 = vec![
        Message::TextMessage {
            id: uuid::Uuid::new_v4(),
            body: "Hello, Actor1!".to_string(),
        },
        Message::IncrementCounter {
            by: 1,
        },
    ];
    let messages_actor2 = vec![
        Message::TextMessage {
            id: uuid::Uuid::new_v4(),
            body: "Hello, Actor2!".to_string(),
        },
        Message::TextMessage {
            id: uuid::Uuid::new_v4(),
            body: "Hey again Actor2!".to_string(),
        },
        Message::IncrementCounter {
            by: 1,
        },
    ];

    // Create vector to store the handles
    let mut actor1_handles = Vec::new();
    let mut actor2_handles = Vec::new();

    // Send messages to actor1
    for message in messages_actor1 {
        let handle = actor1.send(message);
        actor1_handles.push(handle);
    }

    // Send messages to actor2
    for message in messages_actor2 {
        let handle = actor2.send(message);
        actor2_handles.push(handle);
    }

    // Merge the two vectors
    // Cannot use `append` because `join_all` consumes an iterator
    actor1_handles.extend(actor2_handles); // inplace operation, moved value
    let handles = actor1_handles; // move value

    println!("[main] Waiting for producers to finish sending messages");
    // Wait for the producers to finish
    future::join_all(handles).await;

    // Stop the actors
    let stop_actor1 = actor1.stop();
    let stop_actor2 = actor2.stop();

    let _ = tokio::join!(stop_actor1, stop_actor2);

    // Check states of the actors and if channels are closed
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    println!(
        "[main] Actor1 state: '{:?}'",
        actor1.state.lock().unwrap().borrow_mut()
    );
    println!(
        "[main] Actor2 state: '{:?}'",
        actor2.state.lock().unwrap().borrow_mut()
    );

    // Wait for the actors (subscribers) to finish,
    // otherwise the program will exit before for the actors finish processing messages
    // that's because of the sleep in the actor.run() method
    // try commenting out the line below and see what happens
    // the program will exit before the actors finish processing messages
    //
    // Can use tokio::join!, but requires passing values individually
    println!("[main] Waiting for actors to finish processing messages");
    let _ = tokio::join!(actor1_worker, actor2_worker); // Runs forever if channels are not closed

    println!("[main] Done");
    println!(
        "[main] Actor1 state: '{:?}'",
        actor1.state.lock().unwrap().borrow_mut()
    );
    println!(
        "[main] Actor2 state: '{:?}'",
        actor2.state.lock().unwrap().borrow_mut()
    );
}
