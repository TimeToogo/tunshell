import React from "react";
import { Container } from "../layout";
import * as Styled from "./styled";

export const Footer = () => {
  return (
    <Styled.Footer>
      <Container>
        <Styled.Contents>
          <Styled.Credits>
            <span>
              Special thanks to{" "}
              <a href="https://aws.amazon.com/opensource/" target="_blank">
                AWS Open Source
              </a>{" "}
              for sponsoring this project{" "}
            </span>
            <ion-icon name="heart"></ion-icon>
          </Styled.Credits>
        </Styled.Contents>
      </Container>
    </Styled.Footer>
  );
};
