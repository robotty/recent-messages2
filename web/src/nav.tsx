import { Location } from 'history';
import * as React from 'react';
import { Link as RRLink, matchPath, NavLink as RRNavLink, withRouter } from 'react-router-dom';
import {
    Collapse,
    Container,
    Dropdown,
    DropdownItem,
    DropdownMenu,
    DropdownToggle,
    Nav as BsNav,
    Navbar,
    NavbarBrand,
    NavbarToggler,
    NavItem,
    NavLink
} from 'reactstrap';
import { AuthState } from './index';

export class Nav extends React.Component<{ auth: AuthState, location: Location<{}> }, { menuCollapsed: boolean, dropdownOpen: boolean }> {
    constructor(props) {
        super(props);
        this.state = { menuCollapsed: true, dropdownOpen: false };
    }

    render() {
        const toggleCollapsed = () => {
            this.setState((state, props) => {
                return { menuCollapsed: !state.menuCollapsed };
            });
        };
        const toggleDropdown = () => {
            this.setState((state, props) => {
                return { dropdownOpen: !state.dropdownOpen };
            });
        };

        let loginSection;
        let location = this.props.location;

        switch (this.props.auth.type) {
            case 'missing':
                // hide the login button on the /authorized error page. The user should first "Go back", then try again
                if (matchPath(location.pathname, '/authorized')) {
                    loginSection = <NavItem>Logging you in...</NavItem>;
                } else {
                    loginSection = <NavItem>
                        <NavLink tag={RRLink}
                                 to={`/login?returnTo=${encodeURIComponent(location.pathname + location.search + location.hash)}`}><i
                            className="fas fa-sign-in-alt mr-1"/>Login</NavLink>
                    </NavItem>;
                }
                break;
            case 'loading':
                loginSection = <NavItem>Logging you in...</NavItem>;
                break;
            case 'present':
                if (this.props.auth.userDetailsValidating) {
                    loginSection = <NavItem>Checking login status...</NavItem>;
                } else {
                    loginSection =
                        <Dropdown nav isOpen={this.state.dropdownOpen} toggle={toggleDropdown}>
                            <DropdownToggle nav caret>
                                <img className="profile-image mr-1"
                                     src={this.props.auth.userProfileImageUrl}
                                     alt="Profile picture"/> {this.props.auth.userName}
                            </DropdownToggle>
                            <DropdownMenu>
                                <DropdownItem tag={RRLink} to="/settings">Settings</DropdownItem>
                                <DropdownItem tag={RRLink}
                                              to={`/logout?returnTo=${encodeURIComponent(location.pathname + location.search + location.hash)}`}><i
                                    className="fas fa-sign-out-alt mr-1"/>Log Out</DropdownItem>
                            </DropdownMenu>
                        </Dropdown>;
                }
                break;
        }

        return <Navbar expand="lg" className="navbar-themed">
            <Container>
                <NavbarBrand tag={RRNavLink} to="/">recent-messages</NavbarBrand>
                <NavbarToggler onClick={toggleCollapsed}/>
                <Collapse isOpen={!this.state.menuCollapsed} navbar>
                    <BsNav className="mr-auto" navbar>
                        <NavItem><NavLink tag={RRNavLink} to="/" exact>Home</NavLink></NavItem>
                        <NavItem><NavLink tag={RRNavLink} to="/settings">Settings</NavLink></NavItem>
                        <NavItem><NavLink tag={RRNavLink} to="/api">API</NavLink></NavItem>
                    </BsNav>
                    <BsNav navbar>
                        {loginSection}
                    </BsNav>
                </Collapse>
            </Container>
        </Navbar>;
    }
}

export const NavWithRouter = withRouter(Nav);
