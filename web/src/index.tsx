import * as React from 'react';
import * as ReactDOM from 'react-dom';
import { BrowserRouter as Router, Route, Switch } from 'react-router-dom';
import { Container } from 'reactstrap';
import { AuthorizedWithRouter, LoginWithRouter, Logout, revalidateLogin } from './login';
import { NavWithRouter } from './nav';
import { Settings } from './settings';
import { API, Home } from './static';

export interface AuthMissing {
    type: 'missing'
}

export interface AuthLoading {
    type: 'loading'
}

export interface AuthPresent {
    type: 'present',
    accessToken: string,
    validUntil: Date,
    userId: string,
    userLogin: string,
    userName: string
    userProfileImageUrl: string,
    userDetailsValidUntil: Date,
    userDetailsValidating: boolean
}

export type AuthState = AuthMissing | AuthLoading | AuthPresent;

export class App extends React.Component<{}, { auth: AuthState }> {
    private runningTicker: number | undefined;
    constructor(props) {
        super(props);
        this.state = {
            auth: {
                type: 'missing'
            }
        };
        this.runningTicker = undefined;

        this.updateAuthState = this.updateAuthState.bind(this);
    }

    updateAuthState(newAuthState?: AuthState) {
        this.setState((oldState) => {
            // If newAuthState is present we check it for expiration first, and then actually apply a state based on that.
            // If newAuthState is undefined we simply check the existing state for expiration instead of a (different) new one.
            if (newAuthState == null) {
                newAuthState = oldState.auth;
            }

            console.log('got request to update auth state to:', newAuthState);
            if (newAuthState.type === 'present') {
                if (newAuthState.validUntil < new Date()) {
                    // token is irrecoverably expired
                    newAuthState = { type: 'missing' };
                } else {
                    // if this is expired, then it's time for the backend to re-check that the user has not disconnected
                    // the Twitch connection/integration.

                    // note because userDetailsValidUntil (soft expiry) is always expected to be earlier than validUntil (hard expiry)
                    // we only check whether we are close to that (lower) expiry - the revalidateLogin method below
                    // extends both validity periods at the same time
                    let shouldRevalidate = newAuthState.userDetailsValidUntil.getTime() < Date.now();

                    newAuthState = {
                        ...newAuthState,
                        userDetailsValidating: shouldRevalidate
                    };

                    if (shouldRevalidate) {
                        revalidateLogin(newAuthState, this.updateAuthState);
                    }
                }
            }
            console.log('after checking expirations, new auth state is actually going to be:', newAuthState);

            if (newAuthState.type === 'present' || newAuthState.type === 'missing') {
                console.log('updated auth in localStorage');
                window.localStorage.setItem('auth', JSON.stringify(newAuthState));
            }

            return { auth: newAuthState };
        });
    }

    componentDidMount() {
        let storedAuthRaw = window.localStorage.getItem('auth');
        if (storedAuthRaw != null) {
            console.log('loaded auth from localStorage');
            let storedAuth: AuthState = JSON.parse(storedAuthRaw);
            if (storedAuth.type === 'present') {
                storedAuth.validUntil = new Date(storedAuth.validUntil);
                storedAuth.userDetailsValidUntil = new Date(storedAuth.userDetailsValidUntil);
            }
            this.updateAuthState(storedAuth);
        }

        this.runningTicker = setInterval(() => {
            console.log('tick! making sure auth has not expired');
            this.updateAuthState();
        }, 3 * 60 * 1000); // every 3 minutes
    }

    componentWillUnmount() {
        if (this.runningTicker != null) {
            clearInterval(this.runningTicker);
        }
    }

    render() {
        return <Router>
            <NavWithRouter auth={this.state.auth}/>
            <Container className="pt-3">
                <Switch>
                    <Route path="/api">
                        <h1>API</h1>
                        <API/>
                    </Route>
                    <Route path="/settings">
                        <h1>Settings</h1>
                        <Settings auth={this.state.auth} updateAuthState={this.updateAuthState}/>
                    </Route>
                    <Route path="/login">
                        <h1>Login</h1>
                        <LoginWithRouter updateAuthState={this.updateAuthState}/>
                    </Route>
                    <Route path="/authorized">
                        <h1>Login</h1>
                        <AuthorizedWithRouter updateAuthState={this.updateAuthState}/>
                    </Route>
                    <Route path="/logout">
                        <Logout auth={this.state.auth} updateAuthState={this.updateAuthState}/>
                    </Route>
                    <Route exact path="/">
                        <h1>recent-messages Home</h1>
                        <Home/>
                    </Route>
                    <Route path="/">
                        <h1>Not Found</h1>
                        The page you were trying to access does not exist.
                    </Route>
                </Switch>
            </Container>
        </Router>;
    }
}

ReactDOM.render(<App/>, document.getElementById('root'));
