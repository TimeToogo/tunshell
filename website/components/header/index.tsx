import React from "react";
import { Container } from "../layout";
import * as Styled from "./styled";
import { Link } from "../link";

export const Header = () => {
  return (
    <Styled.Header>
      <Container>
        <Styled.Contents>
          <Link href="/">
            <Styled.Logo>
              <img src="/images/logo.svg" />
              <span>
                Tunshell
              </span>
            </Styled.Logo>
          </Link>
          <Styled.Nav>
            <ul>
              <li className="hide-on-mobile">
                <a href="https://github.com/TimeToogo/tunshell#readme">README.md</a>
              </li>
              <li className="hide-on-mobile">
                <Link href="/go">Get started</Link>
              </li>
              <li>
                <a href="https://github.com/TimeToogo/tunshell">
                  <ion-icon name="logo-github"></ion-icon>
                </a>
              </li>
            </ul>
          </Styled.Nav>
        </Styled.Contents>
      </Container>
    </Styled.Header>
  );
};
