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
              <span>{"{> "}Tunshell <em>Beta</em></span>
            </Styled.Logo>
          </Link>
          <Styled.Nav>
            <ul>
              <li>
                <Link href="/how">How it works</Link>
              </li>
              <li>
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
